package web

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"

	"github.com/go-playground/validator/v10"
	"github.com/labstack/echo/v5"
	"github.com/labstack/echo/v5/middleware"
	"github.com/lemonc7/tune-go/assets"
	"github.com/lemonc7/tune-go/config"
	"github.com/lemonc7/tune-go/database"
	"github.com/lemonc7/tune-go/internal/download"
	"github.com/lemonc7/tune-go/internal/mcp"
	"github.com/lemonc7/tune-go/internal/model"
	"github.com/lemonc7/tune-go/internal/provider"
	"github.com/lemonc7/tune-go/internal/provider/kuwo"
	"github.com/lemonc7/tune-go/internal/repo"
	"github.com/lemonc7/tune-go/internal/service"
	"github.com/lemonc7/tune-go/internal/tag"
	"github.com/lemonc7/tune-go/views/components"
)

// Server 保存 HTTP 服务及其需要在退出时释放的资源。
type Server struct {
	echo    *echo.Echo
	logger  *slog.Logger
	service *service.Music
	db      *sql.DB
}

// New 创建应用并完成中间件、音乐源、数据库、下载器和路由的组装。
func New(cfg *config.Config, logger *slog.Logger) (*Server, error) {
	e := echo.New()
	e.Validator = &model.Validator{V: validator.New()}

	e.Use(middleware.RequestLoggerWithConfig(middleware.RequestLoggerConfig{
		LogStatus:   true,
		LogURI:      true,
		LogMethod:   true,
		LogLatency:  true,
		LogRemoteIP: true,
		HandleError: true,
		LogValuesFunc: func(c *echo.Context, v middleware.RequestLoggerValues) error {
			ctx := c.Request().Context()
			attrs := []slog.Attr{
				slog.String("method", v.Method),
				slog.Int("status", v.Status),
				slog.String("uri", v.URI),
				slog.Duration("latency", v.Latency),
				slog.String("ip", v.RemoteIP),
			}

			var level slog.Level
			if v.Error != nil || v.Status >= 500 {
				level = slog.LevelError
				if v.Error != nil {
					attrs = append(attrs, slog.String("error", v.Error.Error()))
				}
			} else if v.Status >= 400 {
				level = slog.LevelWarn
			} else {
				level = slog.LevelInfo
			}

			logger.LogAttrs(ctx, level, "HTTP_REQUEST", attrs...)
			return nil
		},
	}))
	e.Use(middleware.Recover())

	e.HTTPErrorHandler = func(c *echo.Context, err error) {
		if resp, err := echo.UnwrapResponse(c.Response()); err == nil {
			if resp.Committed {
				return
			}
		}

		status := http.StatusInternalServerError
		var msg string
		if he, ok := errors.AsType[*echo.HTTPError](err); ok {
			status = he.Code
			msg = he.Message
		} else {
			msg = err.Error()
		}

		if c.Request().Method == http.MethodHead {
			c.NoContent(status)
			return
		}

		// 如果是音频流，正常返回错误
		if strings.HasPrefix(c.Request().URL.Path, "/stream") {
			c.String(status, msg)
			return
		}

		// 返回错误html
		render(c, http.StatusOK, components.ErrorNotice(msg))
	}

	client := &http.Client{Timeout: cfg.Server.HTTPTimeout}
	downloadClient := &http.Client{Timeout: cfg.Server.DownloadTimeout}

	store, err := database.New(cfg.Database)
	if err != nil {
		return nil, fmt.Errorf("initialize database: %w", err)
	}
	db := store
	queries := repo.New(db)
	downloader := download.New(
		downloadClient,
		cfg.Download.MaxBytes,
	)

	kuwoProvider := kuwo.New(
		client,
		cfg.Providers.Kuwo.SearchURL,
		cfg.Providers.Kuwo.ResolveURL,
		cfg.Providers.Kuwo.LyricURL,
	)

	service := service.NewMusic(
		db,
		queries,
		map[string]provider.Provider{
			kuwoProvider.Name(): kuwoProvider,
		},
		downloader,
		tag.New(client),
		cfg.Download.MaxConcurrent,
		logger,
	)

	if err := service.Start(context.Background()); err != nil {
		db.Close()
		return nil, fmt.Errorf("start download queue: %w", err)
	}

	app := &Server{
		echo:    e,
		logger:  logger,
		service: service,
		db:      db,
	}

	app.initRouter(service, cfg.Server.MCPToken)

	return app, nil
}

// Close 先停止下载 worker，再关闭 SQLite 连接池。
func (s *Server) Close() error {
	s.service.Close()
	return s.db.Close()
}

// ServeHTTP 让 Server 实现标准库 http.Handler 接口。
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	s.echo.ServeHTTP(w, r)
}

// initRouter 注册健康检查、MCP 和 REST API 路由。
func (s *Server) initRouter(service *service.Music, mcpToken string) {
	h := &Handler{Music: service}

	s.echo.GET("/health", func(c *echo.Context) error {
		return c.String(http.StatusOK, "Welcome to TuneGo")
	})

	s.echo.Any("/mcp", echo.WrapHandler(mcp.New(
		service,
		s.logger,
		mcpToken,
	)))

	s.echo.GET("/", h.Home)
	s.echo.GET("/downloads", h.DownloadsPage)
	s.echo.GET("/library", h.LibraryPage)

	s.echo.GET("/search", h.Search)
	s.echo.POST("/resolve", h.Resolve)
	s.echo.POST("/downloads", h.CreateDownload)
	s.echo.GET("/downloads/list", h.DownloadList)
	s.echo.POST("/downloads/:id/pause", h.PauseDownload)
	s.echo.POST("/downloads/:id/resume", h.ResumeDownload)
	s.echo.POST("/downloads/:id/cancel", h.CancelDownload)
	s.echo.POST("/downloads/:id/retry", h.RetryDownload)
	s.echo.GET("/library/items", h.LibraryItems)
	s.echo.POST("/library/sync", h.SyncLibrary)
	s.echo.GET("/stream", h.Stream)

	s.echo.StaticFS("/assets", assets.Assets)
}
