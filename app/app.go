package app

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"log/slog"
	"net/http"

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
	"github.com/lemonc7/tune-go/internal/web"
)

// App 保存 HTTP 服务及其需要在退出时释放的资源。
type App struct {
	echo   *echo.Echo
	logger *slog.Logger
	music  *service.Music
	db     *sql.DB
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

		c.JSON(status, map[string]string{"error": msg})
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

	musicService := service.NewMusic(
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

	if err := musicService.Start(context.Background()); err != nil {
		db.Close()
		return nil, fmt.Errorf("start download queue: %w", err)
	}

	app := &App{
		echo:   e,
		logger: logger,
		music:  musicService,
		db:     db,
	}

	app.initRouter(musicService, cfg.Server.MCPToken)

	return app, nil
}

// Close 先停止下载 worker，再关闭 SQLite 连接池。
func (a *App) Close() error {
	a.music.Close()
	return a.db.Close()
}

// ServeHTTP 让 App 实现标准库 http.Handler 接口。
func (a *App) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	a.echo.ServeHTTP(w, r)
}

// initRouter 注册健康检查、MCP 和 REST API 路由。
func (a *App) initRouter(service *service.Music, mcpToken string) {
	web := &web.Handler{Music: service}

	a.echo.GET("/health", func(c *echo.Context) error {
		return c.String(http.StatusOK, "Welcome to TuneGo")
	})

	a.echo.Any("/mcp", echo.WrapHandler(mcp.New(
		service,
		a.logger,
		mcpToken,
	)))

	a.echo.GET("/", web.Home)
	a.echo.GET("/downloads", web.DownloadsPage)
	a.echo.GET("/library", web.LibraryPage)
	a.echo.GET("/search", web.Search)

	a.echo.POST("/resolve", web.Resolve)
	a.echo.POST("/downloads", web.CreateDownload)
	a.echo.GET("/downloads/list", web.DownloadList)
	a.echo.POST("/downloads/:id/pause", web.PauseDownload)
	a.echo.POST("/downloads/:id/resume", web.ResumeDownload)
	a.echo.POST("/downloads/:id/cancel", web.CancelDownload)
	a.echo.POST("/downloads/:id/retry", web.RetryDownload)
	a.echo.GET("/library/items", web.LibraryItems)
	a.echo.POST("/library/sync", web.SyncLibrary)

	a.echo.StaticFS("/assets", assets.Assets)
}
