package database

import (
	"context"
	"database/sql"
	"errors"
	"path/filepath"
	"testing"
	"time"

	"github.com/lemonc7/tune-go/config"
)

func TestStoreDownloadLifecycleAndLibrarySync(t *testing.T) {
	store, err := New(config.DatabaseConfig{
		Path:         filepath.Join(t.TempDir(), "tune-go.db"),
		MaxOpenConns: 4, MaxIdleConns: 4, BusyTimeout: 5 * time.Second,
	})
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	defer store.Close()

	ctx := context.Background()
	download, created, err := store.CreateDownload(ctx, CreateDownloadParams{
		Provider: "kuwo", TrackID: "228908", Quality: "2000kflac", Name: "晴天",
	})
	if err != nil {
		t.Fatalf("begin download: %v", err)
	}
	if !created {
		t.Fatal("first task should be created")
	}
	if _, err := store.ClaimDownload(ctx, download.ID); err != nil {
		t.Fatalf("claim download: %v", err)
	}
	if changed, err := store.MarkDownloadDownloading(ctx, download.ID, "https://example.com/song.flac"); err != nil || !changed {
		t.Fatalf("mark downloading: %v", err)
	}
	if changed, err := store.MarkDownloadTagging(ctx, download.ID); err != nil || !changed {
		t.Fatalf("mark tagging: %v", err)
	}
	if changed, err := store.MarkDownloadCompleted(ctx, download.ID, "晴天.flac", "/music/晴天.flac", 1024, "audio/flac"); err != nil || !changed {
		t.Fatalf("mark completed: %v", err)
	}

	rows, err := store.ListDownloads(ctx, 10)
	if err != nil {
		t.Fatalf("list downloads: %v", err)
	}
	if len(rows) != 1 || rows[0].Status != "completed" || rows[0].MusicName != "晴天" {
		t.Fatalf("unexpected downloads: %#v", rows)
	}
	duplicate, created, err := store.CreateDownload(ctx, CreateDownloadParams{
		Provider: "kuwo", TrackID: "228908", Quality: "2000kflac", Name: "晴天",
	})
	if err != nil || created || duplicate.ID != download.ID {
		t.Fatalf("duplicate task = %#v, created = %v, err = %v", duplicate, created, err)
	}

	controlled, created, err := store.CreateDownload(ctx, CreateDownloadParams{
		Provider: "kuwo", TrackID: "2", Quality: "flac", Name: "任务控制测试",
	})
	if err != nil || !created {
		t.Fatalf("create controlled task: created = %v, err = %v", created, err)
	}
	if changed, err := store.PauseDownload(ctx, controlled.ID); err != nil || !changed {
		t.Fatalf("pause task: changed = %v, err = %v", changed, err)
	}
	if changed, err := store.ResumeDownload(ctx, controlled.ID); err != nil || !changed {
		t.Fatalf("resume task: changed = %v, err = %v", changed, err)
	}
	if _, err := store.ClaimDownload(ctx, controlled.ID); err != nil {
		t.Fatal(err)
	}
	if changed, err := store.CancelDownload(ctx, controlled.ID); err != nil || !changed {
		t.Fatalf("cancel task: changed = %v, err = %v", changed, err)
	}
	if changed, err := store.RetryDownload(ctx, controlled.ID); err != nil || !changed {
		t.Fatalf("retry task: changed = %v, err = %v", changed, err)
	}
	if deleted, err := store.DeleteDownload(ctx, controlled.ID); err != nil || !deleted {
		t.Fatalf("delete task: deleted = %v, err = %v", deleted, err)
	}
	if _, err := store.GetDownload(ctx, controlled.ID); !errors.Is(err, sql.ErrNoRows) {
		t.Fatalf("deleted task lookup error = %v", err)
	}

	if err := store.SyncLibrary(ctx, "scan-1", []LibraryFileParams{{
		MusicID: &download.MusicID, Path: "/music/晴天.flac", FileName: "晴天.flac",
		Extension: "flac", Size: 1024, Modified: time.Now().UTC().Format(time.RFC3339Nano),
	}}); err != nil {
		t.Fatalf("sync library: %v", err)
	}
	files, err := store.ListLibraryFiles(ctx, 10)
	if err != nil {
		t.Fatalf("list library: %v", err)
	}
	if len(files) != 1 || files[0].FileName != "晴天.flac" {
		t.Fatalf("unexpected library: %#v", files)
	}
}
