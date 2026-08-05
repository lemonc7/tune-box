package app

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"

	"github.com/go-playground/validator/v10"
	"github.com/labstack/echo/v5"
	"github.com/labstack/echo/v5/middleware"
	"github.com/lemonc7/tune-go/api"
	"github.com/lemonc7/tune-go/config"
	"github.com/lemonc7/tune-go/database"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/mcp"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/provider/kuwo"
	"github.com/lemonc7/tune-go/provider/netease"
	"github.com/lemonc7/tune-go/service"
	"github.com/lemonc7/tune-go/tag"
)

// App 保存 HTTP 服务及其需要在退出时释放的资源。
type App struct {
	echo            *echo.Echo
	logger          *slog.Logger
	resourceHandler *api.ResourceHandler
	music           *service.Music
	store           *database.Store
}

// New 创建应用并完成中间件、音乐源、数据库、下载器和路由的组装。
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
		// 响应已经发送时不能再次写入统一错误结构。
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
	store, err := database.New(cfg.Database)
	if err != nil {
		return nil, fmt.Errorf("initialize database: %w", err)
	}
	downloader := download.New(downloadClient, cfg.Download.Dir, cfg.Download.MaxBytes)
	musicService := service.NewMusic(registry, downloader, tag.New(client), store, cfg.Download.Dir, cfg.Download.MaxConcurrent, logger)
	// 启动时同步一次目录，补录手动放入下载目录的音频文件。
	if count, err := musicService.ScanLibrary(context.Background()); err != nil {
		store.Close()
		return nil, fmt.Errorf("scan music library: %w", err)
	} else {
		logger.Info("music library scanned", "files", count)
	}
	if err := musicService.Start(context.Background()); err != nil {
		store.Close()
		return nil, fmt.Errorf("start download queue: %w", err)
	}

	app := &App{
		echo:            e,
		logger:          logger,
		resourceHandler: &api.ResourceHandler{Music: musicService},
		music:           musicService,
		store:           store,
	}
	app.initRouter()

	return app, nil
}

// Close 先停止下载 worker，再关闭 SQLite 连接池。
func (a *App) Close() error {
	a.music.Close()
	return a.store.Close()
}

// ServeHTTP 让 App 实现标准库 http.Handler 接口。
func (a *App) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	a.echo.ServeHTTP(w, r)
}

// initRouter 注册健康检查、MCP 和 REST API 路由。
func (a *App) initRouter() {
	a.echo.GET("/health", func(c *echo.Context) error {
		return c.String(http.StatusOK, "Welcome to TuneGo")
	})

	a.echo.Any("/mcp", echo.WrapHandler(mcp.New(
		a.resourceHandler.Music,
		a.logger,
	)))

	group := a.echo.Group("/api")
	group.GET("/providers", a.resourceHandler.Providers)
	group.GET("/search", a.resourceHandler.Search)
	group.POST("/resolve", a.resourceHandler.Resolve)
	group.POST("/download", a.resourceHandler.Download)
	group.GET("/downloads", a.resourceHandler.Downloads)
	group.POST("/downloads/:id/pause", a.resourceHandler.PauseDownload)
	group.POST("/downloads/:id/resume", a.resourceHandler.ResumeDownload)
	group.POST("/downloads/:id/cancel", a.resourceHandler.CancelDownload)
	group.POST("/downloads/:id/retry", a.resourceHandler.RetryDownload)
	group.DELETE("/downloads/:id", a.resourceHandler.DeleteDownload)
	group.GET("/library", a.resourceHandler.Library)
	group.POST("/library/scan", a.resourceHandler.ScanLibrary)
}
