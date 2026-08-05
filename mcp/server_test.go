package mcp

import (
	"context"
	"io"
	"log/slog"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/service"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

type fakeProvider struct{}

func (fakeProvider) Name() string    { return "kuwo" }
func (fakeProvider) Available() bool { return true }

func (fakeProvider) Search(_ context.Context, query string, page, limit int) (model.SearchResult, error) {
	return model.SearchResult{
		Provider: "kuwo",
		Query:    query,
		Page:     page,
		Limit:    limit,
		Total:    1,
		Tracks: []model.Track{{
			ID:       "228908",
			Provider: "kuwo",
			Name:     "晴天",
			Artists:  []string{"周杰伦"},
			Qualities: []model.Quality{{
				ID:       "2000kflac",
				Label:    "无损",
				Format:   "flac",
				Lossless: true,
			}},
		}},
	}, nil
}

func (fakeProvider) Resolve(_ context.Context, trackID, qualityID string) (model.Resource, error) {
	return model.Resource{Provider: "kuwo", TrackID: trackID, Quality: model.Quality{ID: qualityID}}, nil
}

func (fakeProvider) Metadata(context.Context, string) (model.MusicMetadata, error) {
	return model.MusicMetadata{}, nil
}

func TestStreamableHTTPTools(t *testing.T) {
	logger := slog.New(slog.NewTextHandler(io.Discard, nil))
	music := service.NewMusic(provider.NewRegistry(fakeProvider{}), nil, nil, nil, t.TempDir(), 1, logger)
	httpServer := httptest.NewServer(New(music, logger))
	defer httpServer.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	client := mcp.NewClient(&mcp.Implementation{Name: "tune-go-test", Version: "0.1.0"}, nil)
	session, err := client.Connect(ctx, &mcp.StreamableClientTransport{Endpoint: httpServer.URL}, nil)
	if err != nil {
		t.Fatalf("connect MCP server: %v", err)
	}
	defer session.Close()

	tools, err := session.ListTools(ctx, nil)
	if err != nil {
		t.Fatalf("list tools: %v", err)
	}
	if len(tools.Tools) != 11 {
		t.Fatalf("tool count = %d, want 11", len(tools.Tools))
	}

	result, err := session.CallTool(ctx, &mcp.CallToolParams{
		Name: "search_music",
		Arguments: map[string]any{
			"provider": "kuwo",
			"query":    "周杰伦 晴天",
			"lossless": true,
		},
	})
	if err != nil {
		t.Fatalf("call search_music: %v", err)
	}
	if result.IsError {
		t.Fatalf("search_music returned tool error: %+v", result.Content)
	}
	output, ok := result.StructuredContent.(map[string]any)
	if !ok {
		t.Fatalf("structured content type = %T, want map[string]any", result.StructuredContent)
	}
	searchResult, ok := output["result"].(map[string]any)
	if !ok || searchResult["provider"] != "kuwo" {
		t.Fatalf("unexpected search result: %#v", output)
	}
}
