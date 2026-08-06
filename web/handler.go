package web

import (
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"github.com/labstack/echo/v5"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/service"
	"github.com/lemonc7/tune-go/views/components"
	"github.com/lemonc7/tune-go/views/pages"
)

type Handler struct {
	Music *service.Music
}

func (h *Handler) Home(c *echo.Context) error {
	req := searchRequest(c)
	if req.Provider == "" && req.Query == "" {
		return render(
			c,
			http.StatusOK,
			pages.SearchPage(h.Music.Providers(), req, nil, ""),
		)
	}

	if req.Provider == "" || req.Query == "" {
		const message = "音乐源和关键词不能为空"
		if isHTMXRequest(c) {
			return render(c, http.StatusOK, components.ErrorNotice(message))
		}
		return render(
			c,
			http.StatusOK,
			pages.SearchPage(h.Music.Providers(), req, nil, message),
		)
	}

	result, err := h.Music.Search(c.Request().Context(), req)
	if err != nil {
		if isHTMXRequest(c) {
			return renderError(c, err)
		}
		return render(
			c,
			http.StatusOK,
			pages.SearchPage(h.Music.Providers(), req, nil, err.Error()),
		)
	}
	if isHTMXRequest(c) {
		return render(c, http.StatusOK, pages.SearchResults(result, req.Lossless))
	}
	return render(
		c,
		http.StatusOK,
		pages.SearchPage(h.Music.Providers(), req, &result, ""),
	)
}

func (h *Handler) Search(c *echo.Context) error {
	req := searchRequest(c)

	if req.Provider == "" || req.Query == "" {
		return render(
			c,
			http.StatusOK,
			components.ErrorNotice("音乐源和关键词不能为空"),
		)
	}

	result, err := h.Music.Search(c.Request().Context(), req)
	if err != nil {
		return renderError(c, err)
	}

	return render(
		c,
		http.StatusOK,
		pages.SearchResults(result, req.Lossless),
	)
}

func searchRequest(c *echo.Context) model.SearchRequest {
	providerName := strings.TrimSpace(c.QueryParam("provider"))
	query := strings.TrimSpace(c.QueryParam("q"))
	lossless := c.QueryParam("lossless") == "true"
	// 初次打开搜索页时默认启用无损过滤；提交搜索后以复选框参数为准。
	if providerName == "" && query == "" {
		lossless = true
	}
	return model.SearchRequest{
		Provider: providerName,
		Query:    query,
		Page:     queryInt(c, "page", 1),
		Limit:    queryInt(c, "limit", 20),
		Lossless: lossless,
	}
}

func isHTMXRequest(c *echo.Context) bool {
	request := c.Request()
	return request.Header.Get("HX-Request") == "true" &&
		request.Header.Get("HX-History-Restore-Request") != "true"
}

func (h *Handler) Resolve(c *echo.Context) error {
	req, err := bindDownloadForm(c.Request())
	if err != nil {
		return renderError(c, err)
	}

	if req.Provider == "" ||
		req.TrackID == "" ||
		req.Quality == "" {
		return render(
			c,
			http.StatusOK,
			components.ErrorNotice("解析参数不完整"),
		)
	}

	resource, err := h.Music.Resolve(
		c.Request().Context(),
		req.ResolveRequest,
	)
	if err != nil {
		return renderError(c, err)
	}

	return render(
		c,
		http.StatusOK,
		components.ResolveResource(req, resource),
	)
}

func (h *Handler) CreateDownload(c *echo.Context) error {
	req, err := bindDownloadForm(c.Request())
	if err != nil {
		return renderError(c, err)
	}

	result, err := h.Music.Download(c.Request().Context(), req)
	if err != nil {
		return renderError(c, err)
	}

	c.Response().Header().Set(
		"HX-Trigger",
		"downloadsChanged",
	)

	message := fmt.Sprintf(
		"任务 #%d 已加入下载队列",
		result.DownloadID,
	)
	if result.Duplicate {
		message = fmt.Sprintf(
			"任务 #%d 已经存在",
			result.DownloadID,
		)
	}

	return render(
		c,
		http.StatusOK,
		components.Notice(message),
	)
}

func (h *Handler) DownloadsPage(c *echo.Context) error {
	items, err := h.Music.ListDownloads(
		c.Request().Context(),
		100,
		0,
	)
	if err != nil {
		return err
	}

	return render(
		c,
		http.StatusOK,
		pages.DownloadsPage(items),
	)
}

func (h *Handler) DownloadList(c *echo.Context) error {
	items, err := h.Music.ListDownloads(
		c.Request().Context(),
		100,
		0,
	)
	if err != nil {
		return renderError(c, err)
	}

	return render(
		c,
		http.StatusOK,
		pages.DownloadList(items, true),
	)
}

func (h *Handler) PauseDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return renderError(c, err)
	}

	if _, err := h.Music.PauseDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return renderError(c, err)
	}

	return h.DownloadList(c)
}

func (h *Handler) ResumeDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return renderError(c, err)
	}

	if _, err := h.Music.ResumeDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return renderError(c, err)
	}

	return h.DownloadList(c)
}

func (h *Handler) CancelDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return renderError(c, err)
	}

	if _, err := h.Music.CancelDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return renderError(c, err)
	}

	return h.DownloadList(c)
}

func (h *Handler) RetryDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return renderError(c, err)
	}

	if _, err := h.Music.RetryDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return renderError(c, err)
	}

	return h.DownloadList(c)
}

func downloadID(c *echo.Context) (int64, error) {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		return 0, fmt.Errorf("无效的下载任务 ID")
	}
	return id, nil
}

func queryInt(c *echo.Context, name string, fallback int) int {
	value, err := strconv.Atoi(c.QueryParam(name))
	if err != nil || value < 1 {
		return fallback
	}
	return value
}
