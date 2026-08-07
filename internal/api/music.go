package api

import (
	"database/sql"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"github.com/labstack/echo/v5"
	"github.com/lemonc7/tune-go/internal/model"
	"github.com/lemonc7/tune-go/internal/provider"
	"github.com/lemonc7/tune-go/internal/service"
)

// MusicHandler 处理音乐源、搜索、解析、下载相关的 REST 请求。
type MusicHandler struct {
	Music *service.Music
}

// Providers 返回已经注册的音乐源及其可用状态。
func (m *MusicHandler) Providers(c *echo.Context) error {
	return c.JSON(http.StatusOK, model.Response{
		Code: http.StatusOK,
		Msg:  "ok",
		Data: m.Music.Providers(),
	})
}

// Search 校验查询参数后调用指定 Provider 搜索歌曲。
func (m *MusicHandler) Search(c *echo.Context) error {
	var req model.SearchRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}
	result, err := m.Music.Search(c.Request().Context(), req)
	if err != nil {
		if providerError := mapProviderError(err); providerError != nil {
			return providerError
		}
		return echo.NewHTTPError(http.StatusBadGateway, err.Error()).
			Wrap(fmt.Errorf("search failed: provider = %s, error = %w", req.Provider, err))
	}
	return c.JSON(http.StatusOK, model.Response{
		Code: http.StatusOK,
		Msg:  "ok",
		Data: result,
	})
}

// Resolve 根据歌曲 ID 和音质 ID 获取临时资源地址。
func (m *MusicHandler) Resolve(c *echo.Context) error {
	var req model.ResolveRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}
	resource, err := m.Music.Resolve(c.Request().Context(), req)
	if err != nil {
		if providerError := mapProviderError(err); providerError != nil {
			return providerError
		}
		return echo.NewHTTPError(http.StatusBadGateway, err.Error()).
			Wrap(fmt.Errorf("resolve failed: provider = %s, error = %w", req.Provider, err))
	}
	return c.JSON(http.StatusOK, model.Response{
		Code: http.StatusOK,
		Msg:  "ok",
		Data: resource,
	})
}

// Download 创建异步下载任务，请求不会等待文件下载完成。
func (m *MusicHandler) Download(c *echo.Context) error {
	var req model.DownloadRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	result, err := m.Music.Download(c.Request().Context(), req)
	if err != nil {
		if providerError := mapProviderError(err); providerError != nil {
			return providerError
		}
		return echo.NewHTTPError(http.StatusBadGateway, err.Error()).
			Wrap(fmt.Errorf("download failed: provider = %s, trackId = %s, error = %w", req.Provider, req.TrackID, err))
	}
	return c.JSON(http.StatusAccepted, model.Response{
		Code: http.StatusAccepted,
		Msg:  "download queued",
		Data: result,
	})
}

// PauseDownload 暂停任务并保留断点数据。
func (m *MusicHandler) PauseDownload(c *echo.Context) error {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		return echo.NewHTTPError(http.StatusBadRequest, "invalid download id")
	}
	result, err := m.Music.PauseDownload(c.Request().Context(), id)
	if err != nil {
		return mapDownloadTaskError(err)
	}
	return c.JSON(http.StatusOK, model.Response{Code: http.StatusOK, Msg: "download paused", Data: result})
}

// ResumeDownload 继续已暂停的任务。
func (m *MusicHandler) ResumeDownload(c *echo.Context) error {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		return echo.NewHTTPError(http.StatusBadRequest, "invalid download id")
	}
	result, err := m.Music.ResumeDownload(c.Request().Context(), id)
	if err != nil {
		return mapDownloadTaskError(err)
	}
	return c.JSON(http.StatusOK, model.Response{Code: http.StatusOK, Msg: "download resumed", Data: result})
}

// CancelDownload 取消任务并删除断点数据。
func (m *MusicHandler) CancelDownload(c *echo.Context) error {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		return echo.NewHTTPError(http.StatusBadRequest, "invalid download id")
	}
	result, err := m.Music.CancelDownload(c.Request().Context(), id)
	if err != nil {
		return mapDownloadTaskError(err)
	}
	return c.JSON(http.StatusOK, model.Response{Code: http.StatusOK, Msg: "download cancelled", Data: result})
}

// RetryDownload 将失败或已取消的任务重新入队。
func (m *MusicHandler) RetryDownload(c *echo.Context) error {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		return echo.NewHTTPError(http.StatusBadRequest, "invalid download id")
	}
	result, err := m.Music.RetryDownload(c.Request().Context(), id)
	if err != nil {
		return mapDownloadTaskError(err)
	}
	return c.JSON(http.StatusOK, model.Response{Code: http.StatusOK, Msg: "download queued", Data: result})
}

// DeleteDownload 删除任务记录，不删除已完成的音乐文件。
func (m *MusicHandler) DeleteDownload(c *echo.Context) error {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		return echo.NewHTTPError(http.StatusBadRequest, "invalid download id")
	}
	if err := m.Music.DeleteDownload(c.Request().Context(), id); err != nil {
		return mapDownloadTaskError(err)
	}
	return c.JSON(http.StatusOK, model.Response{Code: http.StatusOK, Msg: "download deleted"})
}

// Downloads 返回最近的下载历史。
func (m *MusicHandler) Downloads(c *echo.Context) error {
	var req model.ListRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}
	result, err := m.Music.ListDownloads(
		c.Request().Context(),
		int64(req.Limit),
		int64(req.Offset),
	)
	if err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, err.Error())
	}
	return c.JSON(http.StatusOK, model.Response{
		Code: http.StatusOK,
		Msg:  "ok",
		Data: result,
	})
}

func (m *MusicHandler) ListSongs(c *echo.Context) error {
	var req model.ListRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	result, err := m.Music.ListSongs(
		c.Request().Context(),
		int64(req.Limit),
		int64(req.Offset),
	)
	if err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, err.Error())
	}
	return c.JSON(http.StatusOK, model.Response{
		Code: http.StatusOK,
		Msg:  "ok",
		Data: result,
	})
}

func (m *MusicHandler) SearchSongs(c *echo.Context) error {
	var req model.SearchSongsRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	result, err := m.Music.SearchSongs(
		c.Request().Context(),
		req.Keyword,
		int64(req.Limit),
	)
	if err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, err.Error())
	}

	return c.JSON(http.StatusOK, model.Response{
		Code: http.StatusOK,
		Msg:  "ok",
		Data: result,
	})
}

// mapProviderError 将音乐源错误转换成对应的 HTTP 状态码。
func mapProviderError(err error) error {
	if errors.Is(err, provider.ErrUnavailable) {
		return echo.NewHTTPError(http.StatusServiceUnavailable, err.Error())
	}
	if strings.Contains(err.Error(), "unknown provider") {
		return echo.NewHTTPError(http.StatusBadRequest, err.Error())
	}
	return nil
}

// mapDownloadTaskError 统一转换五个任务控制接口的状态错误。
func mapDownloadTaskError(err error) error {
	if errors.Is(err, sql.ErrNoRows) {
		return echo.NewHTTPError(http.StatusNotFound, "download task not found")
	}
	if errors.Is(err, service.ErrDownloadState) {
		return echo.NewHTTPError(http.StatusConflict, err.Error())
	}
	return echo.NewHTTPError(http.StatusInternalServerError, err.Error())
}
