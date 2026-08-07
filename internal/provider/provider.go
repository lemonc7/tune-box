package provider

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"strings"

	"github.com/lemonc7/tune-go/internal/model"
)

var ErrUnavailable = errors.New("provider is not configured")

// Provider 统一不同音乐平台的搜索、资源解析和标签元数据能力。
type Provider interface {
	Name() string
	Available() bool
	Search(ctx context.Context, query string, page, limit int) (model.SearchResult, error)
	Resolve(ctx context.Context, trackID, qualityID string) (model.Resource, error)
	Lyrics(ctx context.Context, trackID string) (string, error)
}

// Registry 根据请求中的名称查找对应音乐源。
type Registry struct {
	providers map[string]Provider
}

// NewRegistry 注册服务启动时启用的音乐源。
func NewRegistry(providers ...Provider) *Registry {
	registry := &Registry{providers: make(map[string]Provider, len(providers))}
	for _, item := range providers {
		registry.providers[item.Name()] = item
	}
	return registry
}

// Get 返回可用的音乐源，并区分名称不存在和配置不可用两类错误。
func (r *Registry) Get(name string) (Provider, error) {
	item, ok := r.providers[strings.ToLower(strings.TrimSpace(name))]
	if !ok {
		return nil, fmt.Errorf("unknown provider %q", name)
	}
	if !item.Available() {
		return nil, fmt.Errorf("%s: %w", name, ErrUnavailable)
	}
	return item, nil
}

// Info 是对外展示的音乐源状态。
type Info struct {
	Name      string `json:"name"`
	Available bool   `json:"available"`
}

// List 返回名称稳定排序后的音乐源列表。
func (r *Registry) List() []Info {
	result := make([]Info, 0, len(r.providers))
	for _, item := range r.providers {
		result = append(result, Info{Name: item.Name(), Available: item.Available()})
	}
	sort.Slice(result, func(i, j int) bool { return result[i].Name < result[j].Name })
	return result
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
