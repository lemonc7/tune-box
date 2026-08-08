package kuwo

import (
	"net/http"
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

type roundTripper func(*http.Request) (*http.Response, error)

func (fn roundTripper) RoundTrip(request *http.Request) (*http.Response, error) { return fn(request) }
