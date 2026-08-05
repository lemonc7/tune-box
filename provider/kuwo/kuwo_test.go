package kuwo

import (
	"context"
	"io"
	"net/http"
	"strings"
	"testing"
)

func TestParseQualities(t *testing.T) {
	result := parseQualities("bitrate:128,foo:x;bitrate:320;bitrate:2000", "mp3|flac")
	if len(result) != 3 {
		t.Fatalf("got %d qualities, want 3: %#v", len(result), result)
	}
	if result[2].ID != "2000kflac" || !result[2].Lossless {
		t.Fatalf("FLAC quality not detected: %#v", result)
	}
}

func TestParseQualitiesFormatFallback(t *testing.T) {
	result := parseQualities("", "mp3|ape|flac")
	if len(result) != 2 {
		t.Fatalf("got %d qualities, want 2: %#v", len(result), result)
	}
}

func TestSearchDecodesCurrentKuwoShape(t *testing.T) {
	const body = `{"TOTAL":"1","abslist":[{"ALBUM":"叶惠美","ALBUMID":"1293","ARTIST":"周杰伦","allartistid":"336","DURATION":"269","MUSICRID":"MUSIC_228908","NAME":"晴天","N_MINFO":"level:ff,bitrate:2000,format:flac,size:52.83Mb","web_albumpic_short":"120/a.jpg"}]}`
	client := &http.Client{Transport: roundTripper(func(*http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: http.StatusOK, Body: io.NopCloser(strings.NewReader(body))}, nil
	})}
	result, err := New(client, "https://search.example.test", "https://resolve.example.test").Search(context.Background(), "晴天", 1, 20)
	if err != nil {
		t.Fatal(err)
	}
	if result.Total != 1 || len(result.Tracks) != 1 {
		t.Fatalf("unexpected result: %#v", result)
	}
	track := result.Tracks[0]
	if track.ID != "228908" || len(track.Qualities) != 1 || track.Qualities[0].ID != "2000kflac" {
		t.Fatalf("unexpected track: %#v", track)
	}
}

func TestMetadataBuildsLRCAndCoverURL(t *testing.T) {
	client := &http.Client{Transport: roundTripper(func(request *http.Request) (*http.Response, error) {
		body := `{"code":200,"data":{"lrclist":[{"lineLyric":"故事的小黄花","time":"29.26"},{"lineLyric":"从出生那年就飘着","time":"32.71"}]}}`
		if request.URL.Host == "artistpicserver.kuwo.cn" {
			body = "http://img1.kwcdn.kuwo.cn/star/albumcover/500/cover.jpg"
		}
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})}
	metadata, err := New(client, "search", "resolve").Metadata(context.Background(), "228908")
	if err != nil {
		t.Fatal(err)
	}
	if metadata.Lyrics != "[00:29.26]故事的小黄花\n[00:32.71]从出生那年就飘着" {
		t.Fatalf("unexpected lyrics: %q", metadata.Lyrics)
	}
	if metadata.CoverURL != "https://img1.kwcdn.kuwo.cn/star/albumcover/500/cover.jpg" {
		t.Fatalf("unexpected cover URL: %q", metadata.CoverURL)
	}
}

type roundTripper func(*http.Request) (*http.Response, error)

func (fn roundTripper) RoundTrip(request *http.Request) (*http.Response, error) { return fn(request) }
