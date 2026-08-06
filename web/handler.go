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
	return render(
		c,
		http.StatusOK,
		pages.SearchPage(h.Music.Providers()),
	)
}

func (h *Handler) Search(c *echo.Context) error {
	page := queryInt(c, "page", 1)
	limit := queryInt(c, "limit", 20)

	req := model.SearchRequest{
		Provider: strings.TrimSpace(c.QueryParam("provider")),
		Query:    strings.TrimSpace(c.QueryParam("q")),
		Page:     page,
		Limit:    limit,
		Lossless: c.QueryParam("lossless") == "true",
	}

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
		pages.SearchResults(result),
	)
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

func (h *Handler) LibraryPage(c *echo.Context) error {
	items, err := h.Music.ListLibrary(
		c.Request().Context(),
		500,
	)
	if err != nil {
		return err
	}

	return render(
		c,
		http.StatusOK,
		pages.LibraryPage(items),
	)
}

func (h *Handler) ScanLibrary(c *echo.Context) error {
	if _, err := h.Music.ScanLibrary(
		c.Request().Context(),
	); err != nil {
		return renderError(c, err)
	}

	items, err := h.Music.ListLibrary(
		c.Request().Context(),
		500,
	)
	if err != nil {
		return renderError(c, err)
	}

	return render(
		c,
		http.StatusOK,
		pages.LibraryList(items),
	)
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
