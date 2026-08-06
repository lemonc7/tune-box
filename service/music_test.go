package service

import (
	"bytes"
	"context"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"sync/atomic"
	"testing"
	"time"

	"github.com/lemonc7/tune-go/config"
	"github.com/lemonc7/tune-go/database"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/tag"
)

type queueProvider struct{ resourceURL string }

func (p queueProvider) Name() string    { return "test" }
func (p queueProvider) Available() bool { return true }
func (p queueProvider) Search(context.Context, string, int, int) (model.SearchResult, error) {
	return model.SearchResult{}, nil
}
func (p queueProvider) Resolve(_ context.Context, trackID, quality string) (model.Resource, error) {
	return model.Resource{
		Provider: "test", TrackID: trackID, URL: p.resourceURL,
		Quality: model.Quality{ID: quality, Format: "flac", Lossless: true},
	}, nil
}

func (p queueProvider) Metadata(context.Context, string) (model.MusicMetadata, error) {
	return model.MusicMetadata{Lyrics: "[00:00.00]测试歌词"}, nil
}

func TestDownloadQueueConcurrencyAndDuplicate(t *testing.T) {
	var current, maximum int64
	started := make(chan struct{}, 3)
	release := make(chan struct{})
	resourceServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		now := atomic.AddInt64(&current, 1)
		defer atomic.AddInt64(&current, -1)
		for {
			old := atomic.LoadInt64(&maximum)
			if now <= old || atomic.CompareAndSwapInt64(&maximum, old, now) {
				break
			}
		}
		started <- struct{}{}
		<-release
		w.Header().Set("Content-Type", "audio/flac")
		payload := append([]byte("fLaC\x80\x00\x00\x22"), make([]byte, 34)...)
		w.Write(append(payload, make([]byte, 64)...))
	}))
	defer resourceServer.Close()

	store, err := database.New(config.DatabaseConfig{
		Path: filepath.Join(t.TempDir(), "queue.db"), MaxOpenConns: 4, MaxIdleConns: 4, BusyTimeout: time.Second,
	})
	if err != nil {
		t.Fatal(err)
	}
	logger := slog.New(slog.NewTextHandler(io.Discard, nil))
	downloadDir := t.TempDir()
	music := NewMusic(
		provider.NewRegistry(queueProvider{resourceURL: resourceServer.URL}),
		download.New(resourceServer.Client(), downloadDir, 1024), tag.New(resourceServer.Client()), store, downloadDir, 2, logger,
	)
	if err := music.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	defer func() {
		music.Close()
		store.Close()
	}()

	first, err := music.Download(context.Background(), model.DownloadRequest{
		ResolveRequest: model.ResolveRequest{Provider: "test", TrackID: "1", Quality: "flac"}, Name: "song-1",
	})
	if err != nil {
		t.Fatal(err)
	}
	duplicate, err := music.Download(context.Background(), model.DownloadRequest{
		ResolveRequest: model.ResolveRequest{Provider: "test", TrackID: "1", Quality: "flac"}, Name: "song-1",
	})
	if err != nil || !duplicate.Duplicate || duplicate.DownloadID != first.DownloadID {
		t.Fatalf("duplicate = %#v, err = %v", duplicate, err)
	}
	for id := 2; id <= 3; id++ {
		if _, err := music.Download(context.Background(), model.DownloadRequest{
			ResolveRequest: model.ResolveRequest{Provider: "test", TrackID: strconv.Itoa(id), Quality: "flac"},
			Name:           "song-" + strconv.Itoa(id),
		}); err != nil {
			t.Fatal(err)
		}
	}

	for i := 0; i < 2; i++ {
		select {
		case <-started:
		case <-time.After(2 * time.Second):
			t.Fatal("two downloads did not start")
		}
	}
	select {
	case <-started:
		t.Fatal("third download exceeded configured concurrency")
	case <-time.After(100 * time.Millisecond):
	}
	close(release)

	deadline := time.Now().Add(3 * time.Second)
	for {
		rows, err := music.ListDownloads(context.Background(), 10)
		if err != nil {
			t.Fatal(err)
		}
		completed := 0
		for _, row := range rows {
			if row.Status == "completed" && row.Progress == 100 {
				completed++
			}
		}
		if completed == 3 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("downloads did not complete: %#v", rows)
		}
		time.Sleep(20 * time.Millisecond)
	}
	if atomic.LoadInt64(&maximum) != 2 {
		t.Fatalf("maximum concurrency = %d, want 2", maximum)
	}

	existing, err := music.Download(context.Background(), model.DownloadRequest{
		ResolveRequest: model.ResolveRequest{Provider: "test", TrackID: "1", Quality: "flac"}, Name: "song-1",
	})
	if err != nil || !existing.Duplicate || existing.Status != "completed" || existing.DownloadID != first.DownloadID {
		t.Fatalf("existing library download = %#v, err = %v", existing, err)
	}

	if err := os.Remove(filepath.Join(downloadDir, "song-1.flac")); err != nil {
		t.Fatal(err)
	}
	requeued, err := music.Download(context.Background(), model.DownloadRequest{
		ResolveRequest: model.ResolveRequest{Provider: "test", TrackID: "1", Quality: "flac"}, Name: "song-1",
	})
	if err != nil || requeued.Duplicate || requeued.Status != "pending" || requeued.DownloadID != first.DownloadID {
		t.Fatalf("missing library download = %#v, err = %v", requeued, err)
	}

	deadline = time.Now().Add(3 * time.Second)
	for {
		row, err := store.GetDownload(context.Background(), first.DownloadID)
		if err != nil {
			t.Fatal(err)
		}
		if row.Status == "completed" {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("missing file was not downloaded again: %#v", row)
		}
		time.Sleep(20 * time.Millisecond)
	}
	content, err := os.ReadFile(filepath.Join(downloadDir, "song-1.flac"))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Contains(content, []byte("LYRICS=[00:00.00]测试歌词")) {
		t.Fatal("completed FLAC does not contain embedded LRC lyrics")
	}
}
