package config

import (
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/ilyakaznacheev/cleanenv"
)

// Config 汇总服务、下载器和音乐源的全部配置。
type Config struct {
	Server    ServerConfig    `yaml:"server"`
	Download  DownloadConfig  `yaml:"download"`
	Providers ProvidersConfig `yaml:"providers"`
}

// ServerConfig 定义监听端口以及两类 HTTP 操作的超时时间。
type ServerConfig struct {
	Port            int           `yaml:"port" env:"PORT" env-default:"8080"`
	HTTPTimeout     time.Duration `yaml:"http_timeout" env:"HTTP_TIMEOUT" env-default:"30s"`
	DownloadTimeout time.Duration `yaml:"download_timeout" env:"DOWNLOAD_TIMEOUT" env-default:"2m"`
}

// DownloadConfig 控制文件保存位置和允许下载的单文件上限。
type DownloadConfig struct {
	Dir      string `yaml:"dir" env:"DOWNLOAD_DIR" env-default:"./downloads"`
	MaxBytes int64  `yaml:"max_bytes" env:"MAX_DOWNLOAD_BYTES" env-default:"104857600"`
}

// ProvidersConfig 保存各音乐平台的接口配置。
type ProvidersConfig struct {
	Kuwo    KuwoConfig    `yaml:"kuwo"`
	Netease NeteaseConfig `yaml:"netease"`
}

// KuwoConfig 包含酷我搜索和资源解析接口。
type KuwoConfig struct {
	SearchURL  string `yaml:"search_url" env:"KUWO_SEARCH_URL"`
	ResolveURL string `yaml:"resolve_url" env:"KUWO_RESOLVE_URL"`
}

// NeteaseConfig 使用自建 API 搜索，使用扩展接口解析下载地址。
type NeteaseConfig struct {
	BaseURL            string `yaml:"base_url" env:"NETEASE_API_BASE_URL"`
	Cookie             string `yaml:"cookie" env:"NETEASE_COOKIE"`
	ExtendedResolveURL string `yaml:"extended_resolve_url" env:"NETEASE_EXTENDED_RESOLVE_URL"`
}

func Load(path string) (*Config, error) {
	var cfg Config
	if err := cleanenv.ReadConfig(path, &cfg); err != nil {
		return nil, fmt.Errorf("read config: %w", err)
	}

	if err := cfg.validate(); err != nil {
		return nil, err
	}

	return &cfg, nil
}

func (c Config) validate() error {
	if c.Server.Port < 1 || c.Server.Port > 65535 {
		return fmt.Errorf("server.port/PORT must be between 1 and 65535")
	}
	if c.Server.HTTPTimeout <= 0 {
		return fmt.Errorf("server.http_timeout/HTTP_TIMEOUT must be positive")
	}
	if c.Server.DownloadTimeout <= 0 {
		return fmt.Errorf("server.download_timeout/DOWNLOAD_TIMEOUT must be positive")
	}
	if strings.TrimSpace(c.Download.Dir) == "" {
		return fmt.Errorf("download.dir/DOWNLOAD_DIR is required")
	}
	if c.Download.MaxBytes <= 0 {
		return fmt.Errorf("download.max_bytes/MAX_DOWNLOAD_BYTES must be positive")
	}
	if c.Providers.Netease.BaseURL == "" {
		return fmt.Errorf("providers.netease.base_url/NETEASE_API_BASE_URL is required")
	}
	return nil
}

// Address 将纯端口转换成 Echo 所需的监听地址格式。
func (c ServerConfig) Address() string {
	return ":" + strconv.Itoa(c.Port)
}
