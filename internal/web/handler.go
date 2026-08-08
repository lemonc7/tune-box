package web

import (
	"fmt"
	"net/http"
	"strconv"

	"github.com/labstack/echo/v5"
	"github.com/lemonc7/tune-go/internal/model"
	"github.com/lemonc7/tune-go/internal/service"
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
		pages.SearchPage(
			h.Music.Providers(),
			model.SearchRequest{Lossless: true},
			nil,
			"",
		),
	)
}

func (h *Handler) Search(c *echo.Context) error {
	var req model.SearchRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(
			http.StatusBadRequest,
			"bad request",
		).Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	result, err := h.Music.Search(c.Request().Context(), req)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"search failed",
		).Wrap(err)
	}

	// 如果请求是由无限滚动触发的
	if c.Request().Header.Get("HX-Trigger") == "search-more" {
		// 仅返回卡片列表
		return render(
			c,
			http.StatusOK,
			pages.TrackList(
				result,
				req.Lossless,
			))
	}

	// 表单提交/首次加载：返回完整的带头部的结果
	return render(
		c,
		http.StatusOK,
		pages.SearchResults(result, req.Lossless),
	)
}

func (h *Handler) Resolve(c *echo.Context) error {
	var req model.DownloadRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(
			http.StatusBadGateway,
			"bad request",
		).Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	resource, err := h.Music.Resolve(
		c.Request().Context(),
		req.ResolveRequest,
	)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"resolve failed",
		).Wrap(err)
	}

	return render(
		c,
		http.StatusOK,
		components.ResolveResource(req, resource),
	)
}

func (h *Handler) CreateDownload(c *echo.Context) error {
	var req model.DownloadRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(
			http.StatusBadRequest,
			"bad request",
		).Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	result, err := h.Music.Download(c.Request().Context(), req)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"download failed",
		).Wrap(err)
	}

	c.Response().Header().Set(
		"HX-Trigger",
		"downloadsChanged",
	)

	message := "已加入下载队列"
	if result.Duplicate {
		message = "下载任务已存在"
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
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"list downloads failed",
		).Wrap(err)
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
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"list downloads failed",
		).Wrap(err)
	}

	return render(
		c,
		http.StatusOK,
		pages.DownloadsList(items),
	)
}

func (h *Handler) PauseDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return err
	}

	if _, err := h.Music.PauseDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"pause download failed",
		).Wrap(err)
	}

	return h.DownloadList(c)
}

func (h *Handler) ResumeDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return err
	}

	if _, err := h.Music.ResumeDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"resume download failed",
		).Wrap(err)
	}

	return h.DownloadList(c)
}

func (h *Handler) CancelDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return err
	}

	if _, err := h.Music.CancelDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"cancel download failed",
		).Wrap(err)
	}

	return h.DownloadList(c)
}

func (h *Handler) RetryDownload(c *echo.Context) error {
	id, err := downloadID(c)
	if err != nil {
		return err
	}

	if _, err := h.Music.RetryDownload(
		c.Request().Context(),
		id,
	); err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"retry download failed",
		).Wrap(err)
	}

	return h.DownloadList(c)
}

func (h *Handler) SyncLibrary(c *echo.Context) error {
	if err := h.Music.SyncLibrary(c.Request().Context()); err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"sync library failed",
		).Wrap(err)
	}
	return c.NoContent(http.StatusOK)
}

func (h *Handler) LibraryPage(c *echo.Context) error {
	ctx := c.Request().Context()
	items, err := h.Music.SearchSongs(ctx, "", 0, 20)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"load songs failed",
		).Wrap(err)
	}

	total, err := h.Music.CountSongs(c.Request().Context())
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"get songs count failed",
		).Wrap(err)
	}

	return render(
		c,
		http.StatusOK,
		pages.LibraryPage(items, int(total)))
}

func (h *Handler) LibraryItems(c *echo.Context) error {
	var req model.SearchSongsRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(
			http.StatusBadRequest,
			"bad request",
		).Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	items, err := h.Music.SearchSongs(
		c.Request().Context(),
		req.Keyword,
		int64(req.Offset),
		int64(req.Limit),
	)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"search songs failed",
		)
	}

	return render(
		c,
		http.StatusOK,
		pages.LibraryRows(
			items,
			req.Keyword,
			req.Offset,
		),
	)
}

func downloadID(c *echo.Context) (int64, error) {
	q := c.Param("id")
	id, err := strconv.ParseInt(q, 10, 64)
	if err != nil || id < 1 {
		return 0, echo.NewHTTPError(
			http.StatusBadRequest,
			fmt.Sprintf("invalid download id: %s", q),
		)
	}
	return id, nil
}
