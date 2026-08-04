package api

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"strings"

	"github.com/labstack/echo/v5"
	"github.com/labstack/echo/v5/middleware"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/mcp"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
)

// Server 组合音乐源注册表、下载器和 Echo 路由。
type Server struct {
	registry   *provider.Registry
	downloader *download.Downloader
	logger     *slog.Logger
	echo       *echo.Echo
}

// response 统一所有接口的外层响应格式。
type response struct {
	Code int    `json:"code"`
	Msg  string `json:"msg"`
	Data any    `json:"data,omitempty"`
}

// New 创建 Echo 服务并安装请求日志、异常恢复和统一错误处理。
func New(registry *provider.Registry, downloader *download.Downloader, logger *slog.Logger) *Server {
	e := echo.New()
	e.Logger = logger
	server := &Server{registry: registry, downloader: downloader, logger: logger, echo: e}
	e.HTTPErrorHandler = server.httpError
	e.Use(server.logRequest, middleware.Recover())
	server.routes()
	return server
}

// Handler 暴露标准库 http.Handler，方便启动服务和编写测试。
func (s *Server) Handler() http.Handler {
	return s.echo
}

func (s *Server) routes() {
	s.echo.GET("/healthz", s.health)
	// MCP 与 REST API 共用同一套 Provider 和下载器。
	s.echo.Any("/mcp", echo.WrapHandler(mcp.New(s.registry, s.downloader, s.logger)))
	s.echo.GET("/api/v1/providers", s.providers)
	s.echo.GET("/api/v1/search", s.search)
	s.echo.POST("/api/v1/resolve", s.resolve)
	s.echo.POST("/api/v1/download", s.download)
}

func (s *Server) health(c *echo.Context) error {
	return writeJSON(c, http.StatusOK, response{Code: 200, Msg: "ok"})
}

func (s *Server) providers(c *echo.Context) error {
	return writeJSON(c, http.StatusOK, response{Code: 200, Msg: "ok", Data: s.registry.List()})
}

func (s *Server) search(c *echo.Context) error {
	name := c.QueryParam("provider")
	query := strings.TrimSpace(c.QueryParam("q"))
	if query == "" {
		return writeError(c, http.StatusBadRequest, "q is required")
	}
	page, err := positiveInt(c.QueryParam("page"), 1, 10000)
	if err != nil {
		return writeError(c, http.StatusBadRequest, "invalid page")
	}
	limit, err := positiveInt(c.QueryParam("limit"), 20, 100)
	if err != nil {
		return writeError(c, http.StatusBadRequest, "invalid limit")
	}
	item, err := s.registry.Get(name)
	if err != nil {
		return writeProviderError(c, err)
	}

	result, err := item.Search(c.Request().Context(), query, page, limit)
	if err != nil {
		s.logger.Error("search failed", "provider", name, "error", err)
		return writeError(c, http.StatusBadGateway, err.Error())
	}
	if c.QueryParam("lossless") == "true" {
		// 过滤发生在平台搜索之后，因此不会改变平台原本的分页参数。
		result = provider.FilterLossless(result)
	}
	return writeJSON(c, http.StatusOK, response{Code: 200, Msg: "ok", Data: result})
}

func (s *Server) resolve(c *echo.Context) error {
	var request model.ResolveRequest
	if err := decodeJSON(c.Request(), &request); err != nil {
		return writeError(c, http.StatusBadRequest, err.Error())
	}
	resource, err := s.resolveResource(c, request)
	if err != nil {
		return s.returnResolveError(c, request.Provider, err)
	}
	return writeJSON(c, http.StatusOK, response{Code: 200, Msg: "ok", Data: resource})
}

// download 先通过 Provider 获取临时资源地址，再由下载器安全落盘。
func (s *Server) download(c *echo.Context) error {
	var request model.DownloadRequest
	if err := decodeJSON(c.Request(), &request); err != nil {
		return writeError(c, http.StatusBadRequest, err.Error())
	}
	resource, err := s.resolveResource(c, request.ResolveRequest)
	if err != nil {
		return s.returnResolveError(c, request.Provider, err)
	}
	result, err := s.downloader.Download(c.Request().Context(), resource, request.FileName)
	if err != nil {
		s.logger.Error("download failed", "provider", request.Provider, "trackId", request.TrackID, "error", err)
		return writeError(c, http.StatusBadGateway, err.Error())
	}
	return writeJSON(c, http.StatusOK, response{Code: 200, Msg: "download completed", Data: result})
}

func (s *Server) resolveResource(c *echo.Context, request model.ResolveRequest) (model.Resource, error) {
	if strings.TrimSpace(request.TrackID) == "" {
		return model.Resource{}, fmt.Errorf("trackId is required")
	}
	if strings.TrimSpace(request.Quality) == "" {
		return model.Resource{}, fmt.Errorf("quality is required")
	}
	item, err := s.registry.Get(request.Provider)
	if err != nil {
		return model.Resource{}, err
	}
	return item.Resolve(c.Request().Context(), request.TrackID, request.Quality)
}

func (s *Server) returnResolveError(c *echo.Context, name string, err error) error {
	if errors.Is(err, provider.ErrUnavailable) || strings.Contains(err.Error(), "unknown provider") {
		return writeProviderError(c, err)
	}
	s.logger.Error("resolve failed", "provider", name, "error", err)
	return writeError(c, http.StatusBadGateway, err.Error())
}

func writeProviderError(c *echo.Context, err error) error {
	status := http.StatusBadRequest
	if errors.Is(err, provider.ErrUnavailable) {
		status = http.StatusServiceUnavailable
	}
	return writeError(c, status, err.Error())
}

func decodeJSON(r *http.Request, target any) error {
	// 限制请求体大小并拒绝未知字段，避免错误参数被静默忽略。
	decoder := json.NewDecoder(io.LimitReader(r.Body, 1024*1024+1))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return fmt.Errorf("invalid JSON: %w", err)
	}
	return nil
}

func positiveInt(raw string, fallback, maximum int) (int, error) {
	if raw == "" {
		return fallback, nil
	}
	value, err := strconv.Atoi(raw)
	if err != nil || value <= 0 || value > maximum {
		return 0, errors.New("out of range")
	}
	return value, nil
}

func writeError(c *echo.Context, status int, message string) error {
	return writeJSON(c, status, response{Code: status, Msg: message})
}

func writeJSON(c *echo.Context, status int, value any) error {
	c.Response().Header().Set("X-Content-Type-Options", "nosniff")
	return c.JSON(status, value)
}

func (s *Server) logRequest(next echo.HandlerFunc) echo.HandlerFunc {
	return func(c *echo.Context) error {
		s.logger.Info("request", "method", c.Request().Method, "path", c.Request().URL.Path)
		return next(c)
	}
}

// httpError 将 Echo 路由错误和 panic 转换成项目统一的 JSON 响应。
func (s *Server) httpError(c *echo.Context, err error) {
	status := http.StatusInternalServerError
	message := "internal server error"
	var httpErr *echo.HTTPError
	if errors.As(err, &httpErr) {
		status = httpErr.Code
		if httpErr.Message != "" {
			message = httpErr.Message
		}
	} else {
		s.logger.Error("request failed", "error", err)
	}
	underlying, _ := echo.UnwrapResponse(c.Response())
	if underlying == nil || !underlying.Committed {
		_ = writeError(c, status, message)
	}
}
