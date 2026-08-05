package app

import (
	"context"
	"errors"
	"log/slog"
	"net/http"

	"github.com/go-playground/validator/v10"
	"github.com/labstack/echo/v5"
	"github.com/labstack/echo/v5/middleware"
	"github.com/lemonc7/tune-go/api"
	"github.com/lemonc7/tune-go/config"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/mcp"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/provider/kuwo"
	"github.com/lemonc7/tune-go/provider/netease"
)

type App struct {
	echo            *echo.Echo
	logger          *slog.Logger
	resourceHandler *api.ResourceHandler
}

func New(cfg *config.Config, logger *slog.Logger) (*App, error) {
	e := echo.New()
	e.Validator = &model.Validator{V: validator.New()}

	e.Use(middleware.RequestID())
	e.Use(middleware.RequestLoggerWithConfig(middleware.RequestLoggerConfig{
		LogStatus:   true,
		LogURI:      true,
		HandleError: true,
		LogValuesFunc: func(c *echo.Context, v middleware.RequestLoggerValues) error {
			if v.Error == nil {
				logger.LogAttrs(context.Background(), slog.LevelInfo, "REQUEST",
					slog.String("uri", v.URI),
					slog.Int("status", v.Status),
				)
			} else {
				logger.LogAttrs(context.Background(), slog.LevelError, "REQUEST_ERROR",
					slog.String("uri", v.URI),
					slog.Int("status", v.Status),
					slog.String("err", v.Error.Error()),
				)
			}
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

		c.JSON(status, model.Response{
			Code: status,
			Msg:  msg,
		})
	}

	client := &http.Client{Timeout: cfg.Server.HTTPTimeout}
	downloadClient := &http.Client{Timeout: cfg.Server.DownloadTimeout}
	registry := provider.NewRegistry(
		kuwo.New(
			client,
			cfg.Providers.Kuwo.SearchURL,
			cfg.Providers.Kuwo.ResolveURL,
		),
		netease.New(
			client,
			cfg.Providers.Netease.BaseURL,
			cfg.Providers.Netease.Cookie,
			cfg.Providers.Netease.ExtendedResolveURL,
		),
	)

	app := &App{
		echo:   e,
		logger: logger,
		resourceHandler: &api.ResourceHandler{
			Registry: registry,
			Downloader: download.New(
				downloadClient,
				cfg.Download.Dir,
				cfg.Download.MaxBytes,
			),
		},
	}
	app.initRouter()

	return app, nil
}

func (a *App) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	a.echo.ServeHTTP(w, r)
}

func (a *App) initRouter() {
	a.echo.GET("/health", func(c *echo.Context) error {
		return c.String(http.StatusOK, "Welcome to TuneGo")
	})

	a.echo.Any("/mcp", echo.WrapHandler(mcp.New(
		a.resourceHandler.Registry,
		a.resourceHandler.Downloader,
		a.logger,
	)))

	group := a.echo.Group("/api")
	group.GET("/providers", a.resourceHandler.Providers)
	group.GET("/search", a.resourceHandler.Search)
	group.POST("/resolve", a.resourceHandler.Resolve)
	group.POST("/download", a.resourceHandler.Download)
}
