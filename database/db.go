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

const dbPath = "data/data.db"

func New(cfg config.DatabaseConfig) (*sql.DB, error) {
	dbName, err := filepath.Abs(dbPath)
	if err != nil {
		return nil, fmt.Errorf("invalid db path: %w", err)
	}

	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
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
	db.SetMaxOpenConns(cfg.MaxOpenConns)
	db.SetMaxIdleConns(cfg.MaxIdleConns)
	db.SetConnMaxLifetime(cfg.ConnMaxLifetime)
	db.SetConnMaxIdleTime(cfg.ConnMaxIdleTime)

	if err := migrateUp(db, dbName); err != nil {
		db.Close()
		return nil, err
	}

	return db, nil
}

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
