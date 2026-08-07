package config

import (
	"fmt"
	"net/url"
	"path/filepath"
	"strconv"
	"time"

	"github.com/ilyakaznacheev/cleanenv"
)

// Config 汇总服务、下载器和音乐源的全部配置。
type Config struct {
	Server    ServerConfig    `yaml:"server"`
	Download  DownloadConfig  `yaml:"download"`
	Providers ProvidersConfig `yaml:"providers"`
	Database  DatabaseConfig  `yaml:"database"`
}

// ServerConfig 定义监听端口以及两类 HTTP 操作的超时时间。
type ServerConfig struct {
	Port            int           `yaml:"port" env:"PORT" env-default:"8080"`
	HTTPTimeout     time.Duration `yaml:"http_timeout" env:"HTTP_TIMEOUT" env-default:"30s"`
	DownloadTimeout time.Duration `yaml:"download_timeout" env:"DOWNLOAD_TIMEOUT" env-default:"30m"`
	MCPToken        string        `yaml:"mcp_token" env:"MCP_TOKEN"`
}

// DownloadConfig 控制文件保存位置和允许下载的单文件上限。
type DownloadConfig struct {
	Dir           string `yaml:"dir" env:"DOWNLOAD_DIR" env-default:"./downloads"`
	MaxBytes      int64  `yaml:"max_bytes" env:"MAX_DOWNLOAD_BYTES" env-default:"104857600"`
	MaxConcurrent int    `yaml:"max_concurrent" env:"MAX_CONCURRENT_DOWNLOADS" env-default:"2"`
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

// DatabaseConfig 定义 SQLite 连接池和锁等待配置。
type DatabaseConfig struct {
	Path            string        `yaml:"path" env:"TUNE_GO_DB_PATH" env-default:"./data/tune-go.db"`
	MaxOpenConns    int           `yaml:"max_open_conns" env:"TUNE_GO_DB_MAX_OPEN_CONNS" env-default:"4"`
	MaxIdleConns    int           `yaml:"max_idle_conns" env:"TUNE_GO_DB_MAX_IDLE_CONNS" env-default:"4"`
	ConnMaxLifetime time.Duration `yaml:"conn_max_lifetime" env:"TUNE_GO_DB_CONN_MAX_LIFETIME" env-default:"0s"`
	ConnMaxIdleTime time.Duration `yaml:"conn_max_idle_time" env:"TUNE_GO_DB_CONN_MAX_IDLE_TIME" env-default:"0s"`
	BusyTimeout     time.Duration `yaml:"busy_timeout" env:"TUNE_GO_DB_BUSY_TIMEOUT" env-default:"5s"`
}

// DSN 返回 modernc.org/sqlite 使用的连接字符串。
func (c DatabaseConfig) DSN(path string) (string, error) {
	busyMilliseconds := c.BusyTimeout.Milliseconds()
	if busyMilliseconds < 1 {
		return "", fmt.Errorf("数据库锁等待时间不能小于 %s", time.Millisecond)
	}

	if path == ":memory:" {
		query := url.Values{
			"mode":    {"memory"},
			"cache":   {"shared"},
			"_pragma": pragmaValues(busyMilliseconds),
		}
		return "file:lensy?" + query.Encode(), nil
	}

	u := &url.URL{Scheme: "file", Path: filepath.ToSlash(path)}
	query := u.Query()
	for _, pragma := range pragmaValues(busyMilliseconds) {
		query.Add("_pragma", pragma)
	}
	u.RawQuery = query.Encode()
	return u.String(), nil
}

func Load(path string) (*Config, error) {
	var cfg Config
	if err := cleanenv.ReadConfig(path, &cfg); err != nil {
		return nil, fmt.Errorf("read config: %w", err)
	}

	if cfg.Server.Port < 1 || cfg.Server.Port > 65535 {
		return nil, fmt.Errorf("server.port must be between 1 and 65535")
	}
	if cfg.Server.HTTPTimeout <= 0 || cfg.Server.DownloadTimeout <= 0 {
		return nil, fmt.Errorf("server timeouts must be positive")
	}
	if cfg.Download.Dir == "" || cfg.Download.MaxBytes <= 0 || cfg.Download.MaxConcurrent <= 0 {
		return nil, fmt.Errorf("download.dir, download.max_bytes and download.max_concurrent must be positive")
	}
	if cfg.Database.Path == "" {
		return nil, fmt.Errorf("database.path is required")
	}
	return &cfg, nil
}

// Address 将纯端口转换成 Echo 所需的监听地址格式。
func (c ServerConfig) Address() string {
	return ":" + strconv.Itoa(c.Port)
}

func pragmaValues(busyMilliseconds int64) []string {
	return []string{
		"foreign_keys(1)",
		"journal_mode(WAL)",
		"synchronous(NORMAL)",
		"busy_timeout(" + strconv.FormatInt(busyMilliseconds, 10) + ")",
	}
}
