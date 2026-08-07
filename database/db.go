package database

import (
	"database/sql"
	"embed"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/golang-migrate/migrate/v4"
	"github.com/golang-migrate/migrate/v4/database/sqlite"
	"github.com/golang-migrate/migrate/v4/source/iofs"
	"github.com/lemonc7/tune-go/config"
	_ "modernc.org/sqlite"
)

//go:embed migrations/*.sql
var migrations embed.FS

const dbPath = "data/tune-go.db"

// New 打开 SQLite、配置连接池、执行迁移并返回数据仓库。
func New(cfg config.Database) (*sql.DB, error) {
	dbName, err := filepath.Abs(dbPath)
	if err != nil {
		return nil, fmt.Errorf("invalid db path: %w", err)
	}

	if err := os.MkdirAll(filepath.Dir(dbName), 0o755); err != nil {
		return nil, fmt.Errorf("create db dir: %w", err)
	}
	dsn, err := cfg.DSN(dbName)
	if err != nil {
		return nil, fmt.Errorf("create SQLite DSN: %w", err)
	}

	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("open SQLite: %w", err)
	}
	// SQLite 使用 WAL 和 busy_timeout 后可以支持少量并发读写，
	// 连接数仍由配置限制，避免产生过多写锁竞争。
	db.SetMaxOpenConns(cfg.MaxOpenConns)
	db.SetMaxIdleConns(cfg.MaxIdleConns)
	db.SetConnMaxLifetime(cfg.ConnMaxLifetime)
	db.SetConnMaxIdleTime(cfg.ConnMaxIdleTime)

	if err := migrateUp(db, dbName); err != nil {
		db.Close()
		return nil, err
	}

	if err := db.Ping(); err != nil {
		db.Close()
		return nil, fmt.Errorf("ping SQLite: %w", err)
	}

	return db, nil
}

// migrateUp 执行嵌入二进制中的全部数据库迁移。
func migrateUp(db *sql.DB, dbName string) error {
	source, err := iofs.New(migrations, "migrations")
	if err != nil {
		return fmt.Errorf("open schema migrations: %w", err)
	}

	driver, err := sqlite.WithInstance(db, &sqlite.Config{
		DatabaseName: dbName,
	})
	if err != nil {
		return fmt.Errorf("create SQLite migrate driver")
	}

	m, err := migrate.NewWithInstance(
		"iofs",
		source,
		"sqlite",
		driver,
	)
	if err != nil {
		return fmt.Errorf("create database migrator: %w", err)
	}

	if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		return fmt.Errorf("migrate database: %w", err)
	}

	return nil
}
