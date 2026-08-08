package tag

import (
	"bytes"
	"context"
	"image"
	"image/color"
	"image/png"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/lemonc7/tune-go/internal/model"
)

func TestEmbedFLAC(t *testing.T) {
	comment, err := buildVorbisComment("original", []string{"TITLE=old title", "REPLAYGAIN_TRACK_GAIN=-3.0 dB"})
	if err != nil {
		t.Fatal(err)
	}
	payload := []byte("fLaC")
	payload = append(payload, 0, 0, 0, 34)
	payload = append(payload, make([]byte, 34)...)
	payload = append(payload, 0x84, byte(len(comment)>>16), byte(len(comment)>>8), byte(len(comment)))
	payload = append(payload, comment...)
	audio := []byte{0xff, 0xf8, 1, 2, 3, 4}
	payload = append(payload, audio...)
	path := filepath.Join(t.TempDir(), "song.flac")
	if err := os.WriteFile(path, payload, 0o644); err != nil {
		t.Fatal(err)
	}

	var cover bytes.Buffer
	imageData := image.NewRGBA(image.Rect(0, 0, 2, 3))
	imageData.Set(0, 0, color.RGBA{R: 255, A: 255})
	if err := png.Encode(&cover, imageData); err != nil {
		t.Fatal(err)
	}
	client := &http.Client{Transport: tagRoundTripper(func(*http.Request) (*http.Response, error) {
		return &http.Response{
			StatusCode:    http.StatusOK,
			Header:        http.Header{"Content-Type": []string{"image/png"}},
			Body:          io.NopCloser(bytes.NewReader(cover.Bytes())),
			ContentLength: int64(cover.Len()),
		}, nil
	})}
	metadata := model.Metadata{
		Name:     "晴天",
		Artists:  []string{"周杰伦"},
		Album:    "叶惠美",
		CoverURL: "https://image.example.test/cover.png",
	}
	if err := New(client).EmbedFLAC(
		context.Background(),
		path,
		metadata,
		"[00:29.26]故事的小黄花",
	); err != nil {
		t.Fatal(err)
	}

	content, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	reader := bytes.NewReader(content[4:])
	var comments []string
	pictures := 0
	for {
		header := make([]byte, 4)
		if _, err := io.ReadFull(reader, header); err != nil {
			t.Fatal(err)
		}
		last := header[0]&0x80 != 0
		length := int(header[1])<<16 | int(header[2])<<8 | int(header[3])
		data := make([]byte, length)
		if _, err := io.ReadFull(reader, data); err != nil {
			t.Fatal(err)
		}
		switch header[0] & 0x7f {
		case 4:
			_, comments, err = parseVorbisComment(data)
			if err != nil {
				t.Fatal(err)
			}
		case 6:
			pictures++
		}
		if last {
			break
		}
	}
	joined := strings.Join(comments, "\n")
	for _, expected := range []string{
		"TITLE=晴天", "ARTIST=周杰伦", "ALBUM=叶惠美",
		"LYRICS=[00:29.26]故事的小黄花", "REPLAYGAIN_TRACK_GAIN=-3.0 dB",
	} {
		if !strings.Contains(joined, expected) {
			t.Fatalf("missing comment %q in %q", expected, joined)
		}
	}
	if strings.Contains(joined, "TITLE=old title") || pictures != 1 {
		t.Fatalf("old title retained or picture count = %d", pictures)
	}
	remaining, err := io.ReadAll(reader)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(remaining, audio) {
		t.Fatalf("audio frames changed: %v", remaining)
	}
}

type tagRoundTripper func(*http.Request) (*http.Response, error)

func (fn tagRoundTripper) RoundTrip(request *http.Request) (*http.Response, error) {
	return fn(request)
}
