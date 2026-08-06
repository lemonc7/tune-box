package netease

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/lemonc7/tune-go/model"
)

const userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36"

// qualities 对应网易接口常用的音质等级。
var qualities = map[string]model.Quality{
	"standard": {ID: "standard", Label: "MP3 128K", Format: "mp3", Bitrate: 128},
	"higher":   {ID: "higher", Label: "MP3 192K", Format: "mp3", Bitrate: 192},
	"exhigh":   {ID: "exhigh", Label: "MP3 320K", Format: "mp3", Bitrate: 320},
	"lossless": {ID: "lossless", Label: "FLAC Lossless", Format: "flac", Bitrate: 2000, Lossless: true},
	"hires":    {ID: "hires", Label: "FLAC Hi-Res", Format: "flac", Bitrate: 3000, Lossless: true},
}

// Provider 使用自建网易 API 搜索歌曲，并使用扩展接口解析下载地址。
type Provider struct {
	client             *http.Client
	baseURL            string
	cookie             string
	extendedURL        string
	extendedRetries    int
	extendedRetryDelay time.Duration
}

// New 创建网易 Provider；baseURL 和 extendedURL 分别承担搜索与下载解析。
func New(client *http.Client, baseURL, cookie, extendedURL string) *Provider {
	return &Provider{
		client: client, baseURL: strings.TrimRight(strings.TrimSpace(baseURL), "/"), cookie: cookie,
		extendedURL: strings.TrimSpace(extendedURL), extendedRetries: 3, extendedRetryDelay: 2 * time.Second,
	}
}

func (p *Provider) Name() string    { return "netease" }
func (p *Provider) Available() bool { return p.baseURL != "" || p.extendedURL != "" }

type audio struct {
	Bitrate int64 `json:"br"`
	Size    int64 `json:"size"`
}

type artist struct {
	ID   int64  `json:"id"`
	Name string `json:"name"`
}

type album struct {
	ID     int64  `json:"id"`
	Name   string `json:"name"`
	PicURL string `json:"picUrl"`
}

type song struct {
	ID       int64    `json:"id"`
	Name     string   `json:"name"`
	Duration int64    `json:"dt"`
	Artists  []artist `json:"ar"`
	Album    album    `json:"al"`
	Low      *audio   `json:"l"`
	Medium   *audio   `json:"m"`
	High     *audio   `json:"h"`
	SQ       *audio   `json:"sq"`
	HR       *audio   `json:"hr"`
}

type searchResponse struct {
	Code   int `json:"code"`
	Result struct {
		Songs []song `json:"songs"`
		Total int    `json:"songCount"`
	} `json:"result"`
}

func (p *Provider) Search(ctx context.Context, query string, page, limit int) (model.SearchResult, error) {
	// cloudsearch 的 offset 是记录偏移量，而不是页码。
	body := map[string]any{
		"keywords": query,
		"limit":    limit,
		"type":     1,
		"offset":   (page - 1) * limit,
	}
	var payload searchResponse
	if err := p.postAPI(ctx, "/cloudsearch", body, &payload); err != nil {
		return model.SearchResult{}, err
	}
	if payload.Code != 0 && payload.Code != 200 {
		return model.SearchResult{}, fmt.Errorf("Netease API returned code %d", payload.Code)
	}

	tracks := make([]model.Track, 0, len(payload.Result.Songs))
	for _, item := range payload.Result.Songs {
		artistNames := make([]string, 0, len(item.Artists))
		artistIDs := make([]string, 0, len(item.Artists))
		for _, entry := range item.Artists {
			artistNames = append(artistNames, entry.Name)
			artistIDs = append(artistIDs, strconv.FormatInt(entry.ID, 10))
		}
		tracks = append(tracks, model.Track{
			ID:         strconv.FormatInt(item.ID, 10),
			Provider:   p.Name(),
			Name:       item.Name,
			Artists:    artistNames,
			ArtistIDs:  artistIDs,
			Album:      item.Album.Name,
			AlbumID:    strconv.FormatInt(item.Album.ID, 10),
			CoverURL:   item.Album.PicURL,
			DurationMS: item.Duration,
			Qualities:  songQualities(item),
		})
	}

	return model.SearchResult{
		Provider: p.Name(),
		Query:    query,
		Page:     page,
		Limit:    limit,
		Total:    payload.Result.Total,
		Tracks:   tracks,
	}, nil
}

func (p *Provider) Resolve(ctx context.Context, trackID, qualityID string) (model.Resource, error) {
	// 网易歌曲 ID 应为十进制整数，提前校验可以避免无效上游请求。
	quality, ok := qualities[qualityID]
	if !ok {
		return model.Resource{}, fmt.Errorf("unsupported Netease quality %q", qualityID)
	}
	if _, err := strconv.ParseInt(trackID, 10, 64); err != nil {
		return model.Resource{}, fmt.Errorf("invalid Netease trackId")
	}

	resourceURL, err := p.resolve(ctx, trackID, quality)
	if err != nil {
		return model.Resource{}, err
	}
	if strings.TrimSpace(resourceURL) == "" {
		return model.Resource{}, fmt.Errorf("Netease returned no resource URL; check account permission and API compatibility")
	}
	return model.Resource{
		Provider: p.Name(),
		TrackID:  trackID,
		Quality:  quality,
		URL:      resourceURL,
		Headers:  p.resourceHeaders(),
	}, nil
}

type lyricPayload struct {
	Code int `json:"code"`
	LRC  struct {
		Lyric string `json:"lyric"`
	} `json:"lrc"`
}

type detailPayload struct {
	Code  int    `json:"code"`
	Songs []song `json:"songs"`
}

// Metadata 从自建网易 API 获取歌曲详情和原始 LRC 时间轴歌词。
func (p *Provider) Metadata(ctx context.Context, trackID string) (model.MusicMetadata, error) {
	metadata := model.MusicMetadata{}
	var lyricPayload lyricPayload
	lyricErr := p.postAPI(ctx, "/lyric", map[string]any{"id": trackID}, &lyricPayload)
	if lyricErr == nil {
		if lyricPayload.Code != 0 && lyricPayload.Code != http.StatusOK {
			lyricErr = fmt.Errorf("Netease lyric API returned code %d", lyricPayload.Code)
		} else {
			metadata.Lyrics = strings.TrimSpace(lyricPayload.LRC.Lyric)
		}
	}

	var detailPayload detailPayload
	detailErr := p.postAPI(ctx, "/song/detail", map[string]any{"ids": trackID}, &detailPayload)
	if detailErr == nil {
		if detailPayload.Code != 0 && detailPayload.Code != http.StatusOK {
			detailErr = fmt.Errorf("Netease song detail API returned code %d", detailPayload.Code)
		} else if len(detailPayload.Songs) == 0 {
			detailErr = errors.New("Netease song detail API returned no song")
		} else {
			item := detailPayload.Songs[0]
			metadata.Name = item.Name
			metadata.Album = item.Album.Name
			metadata.CoverURL = item.Album.PicURL
			for _, entry := range item.Artists {
				metadata.Artists = append(metadata.Artists, entry.Name)
			}
		}
	}
	return metadata, errors.Join(lyricErr, detailErr)
}

func (p *Provider) resolve(ctx context.Context, trackID string, quality model.Quality) (string, error) {
	// 配置扩展服务后固定使用扩展解析；自建 API 主要负责搜索元数据。
	if p.extendedURL != "" {
		return p.resolveExtended(ctx, trackID, quality)
	}
	return p.resolveFromNeteaseAPI(ctx, trackID, quality)
}

func (p *Provider) resolveExtended(ctx context.Context, trackID string, quality model.Quality) (string, error) {
	u, err := url.Parse(p.extendedURL)
	if err != nil {
		return "", fmt.Errorf("parse extended resolve URL: %w", err)
	}
	values := u.Query()
	values.Set("types", "url")
	values.Set("source", "netease")
	values.Set("id", trackID)
	if quality.Lossless {
		// 与原 Java 项目保持一致，br=999 表示请求可用的最高音质。
		values.Set("br", "999")
	} else {
		values.Set("br", strconv.Itoa(quality.Bitrate))
	}
	u.RawQuery = values.Encode()

	// 扩展服务可能短暂返回空地址，因此按 Java 项目的策略最多尝试三次。
	var lastErr error
	for attempt := 1; attempt <= p.extendedRetries; attempt++ {
		var payload struct {
			URL string `json:"url"`
		}
		if err := p.getJSON(ctx, u.String(), &payload); err != nil {
			lastErr = err
		} else if strings.TrimSpace(payload.URL) == "" {
			lastErr = fmt.Errorf("extended resolver returned an empty URL")
		} else {
			return payload.URL, nil
		}
		if attempt < p.extendedRetries {
			timer := time.NewTimer(p.extendedRetryDelay)
			select {
			case <-ctx.Done():
				timer.Stop()
				return "", ctx.Err()
			case <-timer.C:
			}
		}
	}
	return "", fmt.Errorf("extended Netease resolver failed after %d attempts: %w", p.extendedRetries, lastErr)
}

func (p *Provider) resolveFromNeteaseAPI(ctx context.Context, trackID string, quality model.Quality) (string, error) {
	body := map[string]any{"id": trackID}
	if quality.Lossless {
		body["type"] = quality.ID
	} else {
		body["br"] = quality.Bitrate * 1000
	}
	var payload struct {
		Data json.RawMessage `json:"data"`
	}
	if err := p.postAPI(ctx, "/song/download/url", body, &payload); err != nil {
		return "", err
	}
	return resourceURLFromData(payload.Data)
}

func resourceURLFromData(raw json.RawMessage) (string, error) {
	// 不同版本的网易 API 可能返回单个对象或对象数组，这里同时兼容。
	if len(raw) == 0 || string(raw) == "null" {
		return "", nil
	}
	var object struct {
		URL string `json:"url"`
	}
	if err := json.Unmarshal(raw, &object); err == nil && object.URL != "" {
		return object.URL, nil
	}
	var list []struct {
		URL string `json:"url"`
	}
	if err := json.Unmarshal(raw, &list); err == nil {
		for _, item := range list {
			if item.URL != "" {
				return item.URL, nil
			}
		}
		return "", nil
	}
	return "", fmt.Errorf("unsupported Netease resource response")
}

func songQualities(item song) []model.Quality {
	// 只有 size 大于 0 才认为该歌曲实际声明了对应音质文件。
	result := make([]model.Quality, 0, 5)
	if item.Low != nil && item.Low.Size > 0 {
		result = append(result, qualities["standard"])
	}
	if item.Medium != nil && item.Medium.Size > 0 {
		result = append(result, qualities["higher"])
	}
	if item.High != nil && item.High.Size > 0 {
		result = append(result, qualities["exhigh"])
	}
	if item.SQ != nil && item.SQ.Size > 0 {
		result = append(result, qualities["lossless"])
	}
	if item.HR != nil && item.HR.Size > 0 {
		result = append(result, qualities["hires"])
	}
	return result
}

func (p *Provider) postAPI(ctx context.Context, path string, body any, target any) error {
	// 自建网易 API 使用 JSON POST，与当前部署服务的协议保持一致。
	data, err := json.Marshal(body)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, p.baseURL+path, bytes.NewReader(data))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	var raw json.RawMessage
	if err := p.doJSON(req, &raw); err != nil {
		return err
	}
	if err := json.Unmarshal(raw, target); err != nil {
		return fmt.Errorf("decode Netease API response: %w", err)
	}
	return nil
}

func (p *Provider) getJSON(ctx context.Context, endpoint string, target any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return err
	}
	return p.doJSON(req, target)
}

func (p *Provider) doJSON(req *http.Request, target any) error {
	req.Header.Set("User-Agent", userAgent)
	if p.cookie != "" {
		req.Header.Set("Cookie", p.cookie)
	}
	resp, err := p.client.Do(req)
	if err != nil {
		return fmt.Errorf("Netease API request: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("Netease API returned HTTP %d", resp.StatusCode)
	}
	if err := json.NewDecoder(resp.Body).Decode(target); err != nil {
		return fmt.Errorf("decode Netease response: %w", err)
	}
	return nil
}

func (p *Provider) resourceHeaders() map[string]string {
	headers := map[string]string{"User-Agent": userAgent, "Referer": "https://music.163.com/"}
	if p.cookie != "" {
		headers["Cookie"] = p.cookie
	}
	return headers
}
