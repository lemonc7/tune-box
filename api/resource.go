package api

import (
	"errors"
	"fmt"
	"net/http"
	"strings"

	"github.com/labstack/echo/v5"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
)

type ResourceHandler struct {
	Registry   *provider.Registry
	Downloader *download.Downloader
}

func (r *ResourceHandler) Providers(c *echo.Context) error {
	return c.JSON(http.StatusOK, model.Response{
		Code: 200,
		Msg:  "ok",
		Data: r.Registry.List(),
	})
}

func (r *ResourceHandler) Search(c *echo.Context) error {
	var req model.SearchRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}

	if err := c.Validate(&req); err != nil {
		return err
	}

	item, err := r.Registry.Get(req.Provider)
	if err != nil {
		status := http.StatusBadRequest
		if errors.Is(err, provider.ErrUnavailable) {
			status = http.StatusServiceUnavailable
		}
		return echo.NewHTTPError(status, err.Error())
	}

	result, err := item.Search(c.Request().Context(), req.Query, req.Page, req.Limit)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadGateway, err.Error()).
			Wrap(fmt.Errorf("search failed: provider = %s, error = %w", req.Provider, err))
	}

	if req.Lossless {
		// 过滤无损音乐
		result = provider.FilterLossless(result)
	}

	return c.JSON(http.StatusOK, model.Response{
		Code: 200,
		Msg:  "ok",
		Data: result,
	})
}

func (r *ResourceHandler) Resolve(c *echo.Context) error {
	var req model.ResolveRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	item, err := r.Registry.Get(req.Provider)
	if err != nil {
		return resolveError(err, req.Provider)
	}

	resource, err := item.Resolve(c.Request().Context(), req.TrackID, req.Quality)
	if err != nil {
		return resolveError(err, req.Provider)
	}

	return c.JSON(http.StatusOK, model.Response{
		Code: 200,
		Msg:  "ok",
		Data: resource,
	})
}

func (r *ResourceHandler) Download(c *echo.Context) error {
	var req model.DownloadRequest
	if err := c.Bind(&req); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "bad request").Wrap(err)
	}
	if err := c.Validate(&req); err != nil {
		return err
	}

	item, err := r.Registry.Get(req.Provider)
	if err != nil {
		return resolveError(err, req.Provider)
	}
	resource, err := item.Resolve(c.Request().Context(), req.TrackID, req.Quality)
	if err != nil {
		return resolveError(err, req.Provider)
	}

	result, err := r.Downloader.Download(c.Request().Context(), resource, req.FileName)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadGateway, err.Error()).
			Wrap(fmt.Errorf("download failed: provider = %s, trackId = %s, error = %w",
				req.Provider, req.TrackID, err))
	}

	return c.JSON(http.StatusOK, model.Response{
		Code: 200,
		Msg:  "download completed",
		Data: result,
	})
}

func resolveError(err error, name string) error {
	if errors.Is(err, provider.ErrUnavailable) {
		return echo.NewHTTPError(http.StatusBadRequest, err.Error())
	}
	if strings.Contains(err.Error(), "unknown provider") {
		return echo.NewHTTPError(http.StatusServiceUnavailable, err.Error())
	}

	return echo.NewHTTPError(http.StatusBadGateway, err.Error()).
		Wrap(fmt.Errorf("resolve failed: provider = %s, error = %w", name, err))
}
