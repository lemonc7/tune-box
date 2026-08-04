package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadYAMLAndEnvironmentOverride(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.yml")
	content := []byte(`
server:
  port: 9000
  http_timeout: 10s
  download_timeout: 20m
download:
  dir: ./from-yaml
  max_bytes: 2048
providers:
  kuwo:
    search_url: https://search.example.test
    resolve_url: https://resolve.example.test
  netease:
    base_url: https://one.example.test/
    extended_resolve_url: https://music.example.test/api.php
`)
	if err := os.WriteFile(path, content, 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PORT", "9100")
	t.Setenv("DOWNLOAD_DIR", "./from-env")
	t.Setenv("NETEASE_API_BASE_URL", "https://single.example.test/")

	cfg, err := load(path)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Server.Port != 9100 || cfg.Server.Address() != ":9100" || cfg.Download.Dir != "./from-env" {
		t.Fatalf("environment did not override YAML: %#v", cfg)
	}
	if cfg.Providers.Netease.BaseURL != "https://single.example.test" {
		t.Fatalf("unexpected Netease URL: %q", cfg.Providers.Netease.BaseURL)
	}
}

func TestLoadEnvironmentOnlyWhenYAMLMissing(t *testing.T) {
	t.Setenv("LISTEN_ADDR", ":9200")
	t.Setenv("NETEASE_API_BASE_URL", "http://netease.example.test:3000")

	cfg, err := load(filepath.Join(t.TempDir(), "missing.yml"))
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Server.Port != 9200 || cfg.Server.HTTPTimeout == 0 || cfg.Download.MaxBytes == 0 {
		t.Fatalf("unexpected environment-only config: %#v", cfg)
	}
}
