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
	"github.com/lemonc7/tune-go/api"
	"github.com/lemonc7/tune-go/config"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/provider/kuwo"
	"github.com/lemonc7/tune-go/provider/netease"
)

func main() {
	// 启动阶段只负责组装依赖；各音乐源的具体逻辑由 provider 包实现。
	logger := slog.New(slog.NewTextHandler(os.Stdout, nil))
	cfg, err := config.Load()
	if err != nil {
		logger.Error("load config", "error", err)
		os.Exit(1)
	}

	// 搜索和地址解析应该尽快失败，而完整 FLAC 文件下载可能持续较久，
	// 因此分别使用两个超时时间不同的 HTTP 客户端。
	client := &http.Client{Timeout: cfg.Server.HTTPTimeout}
	downloadClient := &http.Client{Timeout: cfg.Server.DownloadTimeout}
	registry := provider.NewRegistry(
		kuwo.New(client, cfg.Providers.Kuwo.SearchURL, cfg.Providers.Kuwo.ResolveURL),
		netease.New(client, cfg.Providers.Netease.BaseURL, cfg.Providers.Netease.Cookie, cfg.Providers.Netease.ExtendedResolveURL),
	)
	handler := api.New(registry, download.New(downloadClient, cfg.Download.Dir, cfg.Download.MaxBytes), logger).Handler()
	// 收到退出信号后，由 Echo 等待正在处理的请求完成再关闭服务。
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()
	start := echo.StartConfig{
		Address: cfg.Server.Address(), HideBanner: true, GracefulTimeout: 10 * time.Second,
		BeforeServeFunc: func(server *http.Server) error {
			server.ReadHeaderTimeout = 5 * time.Second
			server.ReadTimeout = 15 * time.Second
			server.WriteTimeout = cfg.Server.DownloadTimeout + time.Minute
			server.IdleTimeout = 60 * time.Second
			return nil
		},
	}
	logger.Info("server started", "address", cfg.Server.Address(), "downloadDir", cfg.Download.Dir)
	if err := start.Start(ctx, handler); err != nil {
		logger.Error("server stopped unexpectedly", "error", err)
		os.Exit(1)
	}
}
