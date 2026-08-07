package download

import (
	"context"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/lemonc7/tune-go/internal/model"
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
	}, Options{FileName: "song", TaskID: 1})
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
	}, Options{FileName: "song", TaskID: 1})
	if err == nil {
		t.Fatal("expected HTML response to be rejected")
	}
}

func TestResumePartialDownload(t *testing.T) {
	payload := append([]byte("fLaC"), make([]byte, 64)...)
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, ".download-7.part"), payload[:9], 0o644); err != nil {
		t.Fatal(err)
	}
	client := &http.Client{Transport: roundTripper(func(req *http.Request) (*http.Response, error) {
		if req.Header.Get("Range") != "bytes=9-" {
			t.Fatalf("Range = %q, want bytes=9-", req.Header.Get("Range"))
		}
		return &http.Response{
			StatusCode: http.StatusPartialContent,
			Header: http.Header{
				"Content-Type":  []string{"audio/flac"},
				"Content-Range": []string{"bytes 9-67/68"},
			},
			Body: io.NopCloser(strings.NewReader(string(payload[9:]))), ContentLength: int64(len(payload) - 9),
		}, nil
	})}

	var downloaded, total int64
	result, err := New(client, dir, 1024).Download(context.Background(), model.Resource{
		Provider: "test", TrackID: "7", URL: "https://audio.example.test/song.flac",
		Quality: model.Quality{Format: "flac", Lossless: true},
	}, Options{FileName: "resumed", TaskID: 7, Progress: func(current, expected int64) {
		downloaded, total = current, expected
	}})
	if err != nil {
		t.Fatal(err)
	}
	if downloaded != int64(len(payload)) || total != int64(len(payload)) {
		t.Fatalf("progress = %d/%d", downloaded, total)
	}
	content, err := os.ReadFile(result.Path)
	if err != nil {
		t.Fatal(err)
	}
	if string(content) != string(payload) {
		t.Fatal("resumed file content does not match")
	}
}

func TestFinalizeCompletePartialWithoutRequestingResourceAgain(t *testing.T) {
	payload := string(append([]byte("fLaC"), make([]byte, 32)...))
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, ".download-9.part"), []byte(payload), 0o644); err != nil {
		t.Fatal(err)
	}
	client := &http.Client{Transport: roundTripper(func(*http.Request) (*http.Response, error) {
		t.Fatal("completed partial file should not request the resource again")
		return nil, nil
	})}
	processed := false
	result, err := New(client, dir, 1024).Download(context.Background(), model.Resource{
		Provider: "test", TrackID: "9", URL: "https://audio.example.test/song.flac",
		Quality: model.Quality{Format: "flac", Lossless: true},
	}, Options{
		TaskID: 9, FileName: "recovered", ExpectedBytes: int64(len(payload)),
		BeforeFinalize: func(context.Context, string) error {
			processed = true
			return nil
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	if !processed || result.FileName != "recovered.flac" {
		t.Fatalf("processed = %v, result = %#v", processed, result)
	}
}

type roundTripper func(*http.Request) (*http.Response, error)

func (fn roundTripper) RoundTrip(request *http.Request) (*http.Response, error) {
	return fn(request)
}
