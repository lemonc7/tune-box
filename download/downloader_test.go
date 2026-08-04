package download

import (
	"context"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/lemonc7/tune-go/model"
)

func TestDownloadFLAC(t *testing.T) {
	payload := string(append([]byte("fLaC"), make([]byte, 32)...))
	client := &http.Client{Transport: roundTripper(func(*http.Request) (*http.Response, error) {
		return &http.Response{
			StatusCode:    http.StatusOK,
			Header:        http.Header{"Content-Type": []string{"audio/flac"}},
			Body:          io.NopCloser(strings.NewReader(payload)),
			ContentLength: int64(len(payload)),
		}, nil
	})}

	dir := t.TempDir()
	downloader := New(client, dir, 1024)
	result, err := downloader.Download(context.Background(), model.Resource{
		Provider: "test", TrackID: "1", URL: "https://audio.example.test/song.flac",
		Quality: model.Quality{Format: "flac", Lossless: true},
	}, "song")
	if err != nil {
		t.Fatal(err)
	}
	if result.FileName != "song.flac" {
		t.Fatalf("got file name %q", result.FileName)
	}
	if _, err := os.Stat(filepath.Join(dir, "song.flac")); err != nil {
		t.Fatal(err)
	}
}

func TestRejectHTML(t *testing.T) {
	client := &http.Client{Transport: roundTripper(func(*http.Request) (*http.Response, error) {
		return &http.Response{
			StatusCode:    http.StatusOK,
			Header:        http.Header{"Content-Type": []string{"text/html"}},
			Body:          io.NopCloser(strings.NewReader("not audio")),
			ContentLength: 9,
		}, nil
	})}

	downloader := New(client, t.TempDir(), 1024)
	_, err := downloader.Download(context.Background(), model.Resource{
		Provider: "test", TrackID: "1", URL: "https://audio.example.test/error",
		Quality: model.Quality{Format: "flac", Lossless: true},
	}, "song")
	if err == nil {
		t.Fatal("expected HTML response to be rejected")
	}
}

type roundTripper func(*http.Request) (*http.Response, error)

func (fn roundTripper) RoundTrip(request *http.Request) (*http.Response, error) {
	return fn(request)
}
