package provider

import (
	"context"

	"github.com/lemonc7/tune-go/internal/model"
)

// Provider 统一不同音乐平台的搜索、资源解析和标签元数据能力。
type Provider interface {
	Name() string
	Available() bool
	Search(ctx context.Context, query string, page, limit int) (model.SearchResult, error)
	Resolve(ctx context.Context, trackID, qualityID string) (model.Resource, error)
	Lyrics(ctx context.Context, trackID string) (string, error)
}

// FilterLossless 只保留至少包含一种无损音质的歌曲，
// 同时从每首歌曲的音质列表中移除有损格式。
func FilterLossless(result model.SearchResult) model.SearchResult {
	tracks := result.Tracks[:0]
	for _, track := range result.Tracks {
		qualities := track.Qualities[:0]
		for _, quality := range track.Qualities {
			if quality.Lossless {
				qualities = append(qualities, quality)
			}
		}
		if len(qualities) > 0 {
			track.Qualities = qualities
			tracks = append(tracks, track)
		}
	}
	result.Tracks = tracks
	return result
}
