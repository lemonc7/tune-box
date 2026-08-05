package netease

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"
)

func TestResourceURLFromObject(t *testing.T) {
	got, err := resourceURLFromData(json.RawMessage(`{"url":"https://example.test/music.flac"}`))
	if err != nil {
		t.Fatal(err)
	}
	if got != "https://example.test/music.flac" {
		t.Fatalf("got %q", got)
	}
}

func TestResourceURLFromList(t *testing.T) {
	got, err := resourceURLFromData(json.RawMessage(`[{"url":"https://example.test/music.flac"}]`))
	if err != nil {
		t.Fatal(err)
	}
	if got != "https://example.test/music.flac" {
		t.Fatalf("got %q", got)
	}
}

func TestSongQualities(t *testing.T) {
	item := song{SQ: &audio{Size: 10}, HR: &audio{Size: 20}}
	got := songQualities(item)
	if len(got) != 2 || got[0].ID != "lossless" || got[1].ID != "hires" {
		t.Fatalf("unexpected qualities: %#v", got)
	}
}

func TestSearchUsesConfiguredAPI(t *testing.T) {
	requests := 0
	client := &http.Client{Transport: roundTripper(func(request *http.Request) (*http.Response, error) {
		requests++
		if request.URL.Host != "netease.example.test" || request.URL.Path != "/cloudsearch" {
			t.Fatalf("unexpected request URL: %s", request.URL)
		}
		body := `{"code":200,"result":{"songCount":0,"songs":[]}}`
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})}
	p := New(client, "https://netease.example.test/", "", "")

	for range 2 {
		if _, err := p.Search(context.Background(), "test", 1, 20); err != nil {
			t.Fatal(err)
		}
	}
	if requests != 2 {
		t.Fatalf("expected two search requests, got %d", requests)
	}
}

func TestMetadataUsesLyricAndSongDetail(t *testing.T) {
	client := &http.Client{Transport: roundTripper(func(request *http.Request) (*http.Response, error) {
		body := `{"code":200,"lrc":{"lyric":"[00:29.26]故事的小黄花"}}`
		if request.URL.Path == "/song/detail" {
			body = `{"code":200,"songs":[{"id":1,"name":"晴天","ar":[{"id":1,"name":"周杰伦"}],"al":{"id":2,"name":"叶惠美","picUrl":"https://image.example.test/cover.jpg"}}]}`
		}
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})}
	metadata, err := New(client, "https://netease.example.test", "", "").Metadata(context.Background(), "1")
	if err != nil {
		t.Fatal(err)
	}
	if metadata.Name != "晴天" || metadata.Album != "叶惠美" || len(metadata.Artists) != 1 || metadata.Artists[0] != "周杰伦" {
		t.Fatalf("unexpected metadata: %#v", metadata)
	}
	if metadata.Lyrics != "[00:29.26]故事的小黄花" || metadata.CoverURL == "" {
		t.Fatalf("missing lyrics or cover: %#v", metadata)
	}
}

func TestResolveUsesExtendedResolverWithoutCallingNeteaseDownloadAPI(t *testing.T) {
	var extendedCalls, neteaseCalls int
	client := &http.Client{Transport: roundTripper(func(request *http.Request) (*http.Response, error) {
		body := `{"url":"https://cdn.example.test/music.flac"}`
		if request.URL.Host == "resolver.example.test" {
			extendedCalls++
			query := request.URL.Query()
			if query.Get("auth") != "token" || query.Get("types") != "url" || query.Get("source") != "netease" || query.Get("id") != "186016" || query.Get("br") != "999" {
				t.Fatalf("unexpected resolver query: %s", query.Encode())
			}
		} else {
			neteaseCalls++
			body = `{"data":{"url":"https://wrong.example.test/music.flac"}}`
		}
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})}
	p := New(client, "https://netease.example.test", "", "https://resolver.example.test/api.php?auth=token")

	resource, err := p.Resolve(context.Background(), "186016", "lossless")
	if err != nil {
		t.Fatal(err)
	}
	if resource.URL != "https://cdn.example.test/music.flac" || extendedCalls != 1 || neteaseCalls != 0 {
		t.Fatalf("unexpected routing: resource=%#v extendedCalls=%d neteaseCalls=%d", resource, extendedCalls, neteaseCalls)
	}
}

func TestExtendedResolverRetries(t *testing.T) {
	var calls int
	client := &http.Client{Transport: roundTripper(func(request *http.Request) (*http.Response, error) {
		calls++
		status := http.StatusBadGateway
		body := `{}`
		if calls == 3 {
			status = http.StatusOK
			body = `{"url":"https://cdn.example.test/music.flac"}`
		}
		return &http.Response{StatusCode: status, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})}
	p := New(client, "", "", "https://resolver.example.test/api.php")
	p.extendedRetryDelay = 0

	resource, err := p.Resolve(context.Background(), "186016", "lossless")
	if err != nil {
		t.Fatal(err)
	}
	if calls != 3 || resource.URL == "" {
		t.Fatalf("expected third attempt to succeed, calls=%d resource=%#v", calls, resource)
	}
}

type roundTripper func(*http.Request) (*http.Response, error)

func (fn roundTripper) RoundTrip(request *http.Request) (*http.Response, error) { return fn(request) }
