package main

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/labstack/echo/v5"
	"github.com/lemonc7/tune-go/app"
	"github.com/lemonc7/tune-go/config"
)

func main() {
	// 启动阶段只负责组装依赖；各音乐源的具体逻辑由 provider 包实现。
	logger := slog.New(slog.NewTextHandler(os.Stdout, nil))
	cfg, err := config.Load("./config/config.yml")
	if err != nil {
		logger.Error("load config", "error", err)
		os.Exit(1)
	}

	server, err := app.New(cfg, logger)
	if err != nil {
		logger.Error(err.Error())
		os.Exit(1)
	}
	defer server.Close()

	ctx, stop := signal.NotifyContext(
		context.Background(),
		syscall.SIGINT,
		syscall.SIGTERM,
	)
	defer stop()

	start := echo.StartConfig{
		Address:         cfg.Server.Address(),
		HideBanner:      true,
		GracefulTimeout: 10 * time.Second,
		BeforeServeFunc: func(server *http.Server) error {
			server.ReadHeaderTimeout = 5 * time.Second
			server.ReadTimeout = 15 * time.Second
			server.WriteTimeout = cfg.Server.DownloadTimeout + time.Minute
			server.IdleTimeout = 60 * time.Second
			return nil
		},
	}

	if err := start.Start(ctx, server); err != nil {
		logger.Error("server stopped unexpectedly", "error", err)
	}
}
