package web

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/a-h/templ"
	"github.com/labstack/echo/v5"
	"github.com/lemonc7/tune-go/internal/model"
	"github.com/lemonc7/tune-go/internal/service"
	"github.com/lemonc7/tune-go/views/components"
	"github.com/lemonc7/tune-go/views/pages"
)

type Handler struct {
	Music *service.Music
}

func render(
	c *echo.Context,
	status int,
	component templ.Component,
) error {
	c.Response().Header().Set(
		"Content-Type",
		"text/html; charset=utf-8",
	)
	c.Response().WriteHeader(status)

	return component.Render(
		c.Request().Context(),
		c.Response(),
	)
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
	c.Response().Header().Set("HX-Trigger", "librarySyncDone")
	return render(c, http.StatusOK, pages.SyncDone())
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

var streamClient = &http.Client{
	Transport: &http.Transport{
		Proxy: http.ProxyFromEnvironment,
		DialContext: (&net.Dialer{
			Timeout:   10 * time.Second,
			KeepAlive: 10 * time.Second,
		}).DialContext,
		ForceAttemptHTTP2:   true,
		MaxIdleConns:        100,
		IdleConnTimeout:     90 * time.Second,
		TLSHandshakeTimeout: 10 * time.Second,
		// 限制拿到响应头的等待时间，不限制读取body的总时间
		ResponseHeaderTimeout: 10 * time.Second,
	},
}

func (h *Handler) Stream(c *echo.Context) error {
	var req model.StreamRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request")
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	ctx := c.Request().Context()
	// 获取资源URL
	resource, err := h.Music.Resolve(ctx, model.ResolveRequest{
		Provider: req.Provider,
		TrackID:  req.TrackID,
		Quality:  "128kmp3",
	})
	if err != nil {
		return echo.NewHTTPError(
			http.StatusBadGateway,
			"get audio failed",
		).Wrap(err)
	}

	// 构造请求
	request, err := http.NewRequestWithContext(
		ctx,
		http.MethodGet,
		resource.URL,
		nil,
	)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusInternalServerError,
			"create request",
		).Wrap(err)
	}
	for k, v := range resource.Headers {
		request.Header.Set(k, v)
	}
	// 透传range头，支持进度条拖动
	if r := c.Request().Header.Get("Range"); r != "" {
		request.Header.Set("Range", r)
	}

	// 发送请求，使用专用客户端
	resp, err := streamClient.Do(request)
	if err != nil {
		return echo.NewHTTPError(
			http.StatusBadGateway,
			"get audio stream data",
		).Wrap(err)
	}
	defer resp.Body.Close()

	// 组装响应头
	w := c.Response()
	w.Header().Set("Accept-Ranges", "bytes")
	contentType := resp.Header.Get("Content-Type")
	if contentType == "" || contentType == "application/octet-stream" {
		contentType = "audio/mpeg"
	}
	w.Header().Set("Content-Type", contentType)

	// 透传上游的内容长度和范围
	if resp.StatusCode == http.StatusPartialContent {
		w.Header().Set("Content-Range", resp.Header.Get("Content-Range"))
		w.Header().Set("Content-Length", resp.Header.Get("Content-Length"))
		w.WriteHeader(http.StatusPartialContent)
	} else {
		w.Header().Set("Content-Length", resp.Header.Get("Content-Length"))
		// 显示写入200，保证后续发生错误不会重新写header
		w.WriteHeader(http.StatusOK)
	}

	if _, err := io.Copy(w, resp.Body); err != nil {
		// 客户端主动断开
		if isClientDisconnect(err) {
			return nil
		}
		// 记录意外断流日志，header已经写过了，不能再返回错误
		slog.Warn(
			"stream interrupted",
			"trackId", req.TrackID,
			"error", err,
		)
	}
	return nil
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

func isClientDisconnect(err error) bool {
	if errors.Is(err, context.Canceled) {
		return true
	}
	if errors.Is(err, syscall.EPIPE) || errors.Is(err, syscall.ECONNRESET) {
		return true
	}
	if strings.Contains(err.Error(), "broken pipe") {
		return true
	}
	return false
}
