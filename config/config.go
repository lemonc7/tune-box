package config

import (
	"fmt"
	"net/url"
	"path/filepath"
	"strconv"
	"strings"
	"time"
	_ "time/tzdata"

	"github.com/ilyakaznacheev/cleanenv"
)

// Config 汇总服务、下载器和音乐源的全部配置。
type Config struct {
	Server    Server    `yaml:"server"`
	Download  Download  `yaml:"download"`
	Providers Providers `yaml:"providers"`
	Database  Database  `yaml:"database"`
}

func (c *Config) validate() error {
	if err := c.Server.validate(); err != nil {
		return fmt.Errorf("invalid server config: %w", err)
	}
	if err := c.Download.validate(); err != nil {
		return fmt.Errorf("invalid download config: %w", err)
	}
	if err := c.Database.validate(); err != nil {
		return fmt.Errorf("invalid database config: %w", err)
	}
	return nil
}

// Server 定义监听端口以及两类 HTTP 操作的超时时间。
type Server struct {
	Port            int           `yaml:"port" env:"PORT" env-default:"8080"`
	HTTPTimeout     time.Duration `yaml:"http_timeout" env:"HTTP_TIMEOUT" env-default:"30s"`
	DownloadTimeout time.Duration `yaml:"download_timeout" env:"DOWNLOAD_TIMEOUT" env-default:"30m"`
	MCPToken        string        `yaml:"mcp_token" env:"MCP_TOKEN"`
}

func (s *Server) validate() error {
	if s.Port < 1 || s.Port > 65535 {
		return fmt.Errorf("server post must between 1 and 65535")
	}
	if s.HTTPTimeout <= 0 {
		return fmt.Errorf("http timeout must be positive")
	}
	if s.DownloadTimeout <= 0 {
		return fmt.Errorf("download timeout must be positive")
	}
	if strings.TrimSpace(s.MCPToken) == "" {
		return fmt.Errorf("mcp token does not setup")
	}
	return nil
}

// Download 控制文件保存位置和允许下载的单文件上限。
type Download struct {
	MaxBytes      int64 `yaml:"max_bytes" env:"MAX_DOWNLOAD_BYTES" env-default:"104857600"`
	MaxConcurrent int   `yaml:"max_concurrent" env:"MAX_CONCURRENT_DOWNLOADS" env-default:"2"`
}

func (s *Download) validate() error {
	if s.MaxBytes <= 0 {
		return fmt.Errorf("max bytes must be positive")
	}
	if s.MaxConcurrent < 1 {
		return fmt.Errorf("max concurrent must be positive")
	}
	return nil
}

// Providers 保存各音乐平台的接口配置。
type Providers struct {
	Kuwo Kuwo `yaml:"kuwo"`
}

// Kuwo 包含酷我搜索和资源解析接口。
type Kuwo struct {
	SearchURL  string `yaml:"search_url" env:"KUWO_SEARCH_URL"`
	ResolveURL string `yaml:"resolve_url" env:"KUWO_RESOLVE_URL"`
}

// Database 定义 SQLite 连接池和锁等待配置。
type Database struct {
	MaxOpenConns    int           `yaml:"max_open_conns" env:"TUNE_GO_DB_MAX_OPEN_CONNS" env-default:"4"`
	MaxIdleConns    int           `yaml:"max_idle_conns" env:"TUNE_GO_DB_MAX_IDLE_CONNS" env-default:"4"`
	ConnMaxLifetime time.Duration `yaml:"conn_max_lifetime" env:"TUNE_GO_DB_CONN_MAX_LIFETIME" env-default:"0s"`
	ConnMaxIdleTime time.Duration `yaml:"conn_max_idle_time" env:"TUNE_GO_DB_CONN_MAX_IDLE_TIME" env-default:"0s"`
	BusyTimeout     time.Duration `yaml:"busy_timeout" env:"TUNE_GO_DB_BUSY_TIMEOUT" env-default:"5s"`
}

func (d *Database) validate() error {
	if d.MaxOpenConns <= 0 {
		return fmt.Errorf("max open connects must be positive")
	}
	if d.MaxIdleConns <= 0 || d.MaxIdleConns > d.MaxOpenConns {
		return fmt.Errorf("max idle connects must be positive and less than max open connects")
	}
	if d.ConnMaxLifetime < 0 {
		return fmt.Errorf("max life time should not be negative")
	}
	if d.ConnMaxIdleTime < 0 {
		return fmt.Errorf("max idle time should not be negative")
	}
	if d.BusyTimeout <= 0 {
		return fmt.Errorf("busy timeout must be positive")
	}
	return nil
}

// DSN 返回 modernc.org/sqlite 使用的连接字符串。
func (c Database) DSN(path string) (string, error) {
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

	if err := cfg.validate(); err != nil {
		return nil, err
	}

	return &cfg, nil
}

// Address 将纯端口转换成 Echo 所需的监听地址格式。
func (c Server) Address() string {
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
