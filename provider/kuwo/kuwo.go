package kuwo

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"net/http"
	"net/url"
	"strconv"
	"strings"

	"github.com/lemonc7/tune-go/model"
)

const userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36"

const (
	lyricURL = "https://www.kuwo.cn/openapi/v1/www/lyric/getlyric"
	coverAPI = "https://artistpicserver.kuwo.cn/pic.web"
)

// qualities 将对外使用的音质 ID 映射成统一音质模型。
var qualities = map[string]model.Quality{
	"128kmp3":   {ID: "128kmp3", Label: "MP3 128K", Format: "mp3", Bitrate: 128},
	"192kmp3":   {ID: "192kmp3", Label: "MP3 192K", Format: "mp3", Bitrate: 192},
	"320kmp3":   {ID: "320kmp3", Label: "MP3 320K", Format: "mp3", Bitrate: 320},
	"1000kape":  {ID: "1000kape", Label: "APE", Format: "ape", Bitrate: 1000, Lossless: true},
	"2000kflac": {ID: "2000kflac", Label: "FLAC", Format: "flac", Bitrate: 2000, Lossless: true},
}

// Provider 实现酷我音乐的搜索和临时资源地址解析。
type Provider struct {
	client     *http.Client
	searchURL  string
	resolveURL string
}

// New 使用指定的酷我接口创建 Provider。
func New(client *http.Client, searchURL, resolveURL string) *Provider {
	return &Provider{client: client, searchURL: searchURL, resolveURL: resolveURL}
}

func (p *Provider) Name() string    { return "kuwo" }
func (p *Provider) Available() bool { return p.searchURL != "" && p.resolveURL != "" }

type searchResponse struct {
	Total   any           `json:"TOTAL"`
	AbsList []searchTrack `json:"abslist"`
}

type searchTrack struct {
	Album         string `json:"ALBUM"`
	AlbumID       string `json:"ALBUMID"`
	Artist        string `json:"ARTIST"`
	AllArtistID   string `json:"allartistid"`
	Duration      string `json:"DURATION"`
	MusicRID      string `json:"MUSICRID"`
	Name          string `json:"NAME"`
	NMusicInfo    string `json:"N_MINFO"`
	Format        string `json:"FORMAT"`
	AlbumPicShort string `json:"web_albumpic_short"`
}

func (p *Provider) Search(ctx context.Context, query string, page, limit int) (model.SearchResult, error) {
	// 酷我搜索接口使用从 0 开始的页码，因此传入 page-1。
	u, err := url.Parse(p.searchURL)
	if err != nil {
		return model.SearchResult{}, fmt.Errorf("parse Kuwo search URL: %w", err)
	}
	values := u.Query()
	values.Set("client", "kt")
	values.Set("encoding", "utf8")
	values.Set("rformat", "json")
	values.Set("mobi", "1")
	values.Set("vipver", "1")
	values.Set("pn", strconv.Itoa(page-1))
	values.Set("rn", strconv.Itoa(limit))
	values.Set("correct", "1")
	values.Set("all", query)
	values.Set("ft", "music")
	u.RawQuery = values.Encode()

	var payload searchResponse
	if err := p.getJSON(ctx, u.String(), &payload); err != nil {
		return model.SearchResult{}, err
	}

	tracks := make([]model.Track, 0, len(payload.AbsList))
	for _, item := range payload.AbsList {
		id := strings.TrimPrefix(item.MusicRID, "MUSIC_")
		trackQualities := parseQualities(item.NMusicInfo, item.Format)
		tracks = append(tracks, model.Track{
			ID:         id,
			Provider:   p.Name(),
			Name:       item.Name,
			Artists:    splitNonEmpty(item.Artist, "&"),
			ArtistIDs:  splitNonEmpty(item.AllArtistID, "&"),
			Album:      item.Album,
			AlbumID:    item.AlbumID,
			CoverURL:   coverURL(item.AlbumPicShort),
			DurationMS: durationMillis(item.Duration),
			Qualities:  trackQualities,
		})
	}

	return model.SearchResult{
		Provider: p.Name(),
		Query:    query,
		Page:     page,
		Limit:    limit,
		Total:    flexibleInt(payload.Total),
		Tracks:   tracks,
	}, nil
}

type resolveResponse struct {
	Code int `json:"code"`
	Data struct {
		URL     string `json:"url"`
		Format  string `json:"format"`
		Bitrate any    `json:"bitrate"`
	} `json:"data"`
}

func (p *Provider) Resolve(ctx context.Context, trackID, qualityID string) (model.Resource, error) {
	// 只接受搜索结果中声明过的音质，避免把任意参数传给上游。
	quality, ok := qualities[qualityID]
	if !ok {
		return model.Resource{}, fmt.Errorf("unsupported Kuwo quality %q", qualityID)
	}
	if strings.TrimSpace(trackID) == "" {
		return model.Resource{}, fmt.Errorf("trackId is required")
	}

	u, err := url.Parse(p.resolveURL)
	if err != nil {
		return model.Resource{}, fmt.Errorf("parse Kuwo resolve URL: %w", err)
	}
	values := u.Query()
	values.Set("f", "web")
	values.Set("user", "0")
	values.Set("source", "kwplayer_ar_5.0.0.0_B_jiakong_vh.apk")
	values.Set("type", "convert_url_with_sign")
	values.Set("rid", trackID)
	values.Set("br", quality.ID)
	u.RawQuery = values.Encode()

	var payload resolveResponse
	if err := p.getJSON(ctx, u.String(), &payload); err != nil {
		return model.Resource{}, err
	}
	if strings.TrimSpace(payload.Data.URL) == "" {
		return model.Resource{}, fmt.Errorf("Kuwo returned no resource URL; the track may require permission or the endpoint changed")
	}

	return model.Resource{
		Provider: p.Name(),
		TrackID:  trackID,
		Quality:  quality,
		URL:      payload.Data.URL,
		Headers:  map[string]string{"User-Agent": userAgent},
	}, nil
}

type lyricPayload struct {
	Code int `json:"code"`
	Data struct {
		Lines []struct {
			Text string `json:"lineLyric"`
			Time string `json:"time"`
		} `json:"lrclist"`
	} `json:"data"`
}

// Metadata 获取酷我的时间轴歌词和 500px 专辑封面。
func (p *Provider) Metadata(ctx context.Context, trackID string) (model.MusicMetadata, error) {
	metadata := model.MusicMetadata{}
	lyricEndpoint, _ := url.Parse(lyricURL)
	values := lyricEndpoint.Query()
	values.Set("musicId", trackID)
	lyricEndpoint.RawQuery = values.Encode()
	
	var lyricPayload lyricPayload
	var lyricErr error
	if err := p.getJSON(ctx, lyricEndpoint.String(), &lyricPayload); err != nil {
		lyricErr = err
	} else if lyricPayload.Code != http.StatusOK {
		lyricErr = fmt.Errorf("Kuwo lyric API returned code %d", lyricPayload.Code)
	} else {
		var lyrics strings.Builder
		for _, line := range lyricPayload.Data.Lines {
			seconds, err := strconv.ParseFloat(line.Time, 64)
			if err != nil || seconds < 0 {
				continue
			}
			hundredths := int64(math.Round(seconds * 100))
			fmt.Fprintf(&lyrics, "[%02d:%02d.%02d]%s\n", hundredths/6000, hundredths/100%60, hundredths%100, line.Text)
		}
		metadata.Lyrics = strings.TrimSpace(lyrics.String())
	}

	coverEndpoint, _ := url.Parse(coverAPI)
	values = coverEndpoint.Query()
	values.Set("prod", "kwplayer_ar_11.3.4.1")
	values.Set("corp", "kuwo")
	values.Set("source", "kwplayer_ar_11.3.4.1_hw.apk")
	values.Set("type", "rid_pic")
	values.Set("pictype", "url")
	values.Set("content", "list")
	values.Set("size", "500")
	values.Set("rid", trackID)
	values.Set("jfencv", "user,android_id")
	coverEndpoint.RawQuery = values.Encode()

	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, coverEndpoint.String(), nil)
	req.Header.Set("User-Agent", userAgent)
	resp, coverErr := p.client.Do(req)
	if coverErr == nil {
		defer resp.Body.Close()
		if resp.StatusCode < 200 || resp.StatusCode >= 300 {
			coverErr = fmt.Errorf("Kuwo cover API returned HTTP %d", resp.StatusCode)
		} else {
			body, err := io.ReadAll(io.LimitReader(resp.Body, 2049))
			if err != nil {
				coverErr = err
			} else if len(body) > 2048 {
				coverErr = errors.New("Kuwo cover URL is too long")
			} else {
				metadata.CoverURL = strings.TrimSpace(string(body))
				if after, ok := strings.CutPrefix(metadata.CoverURL, "http://"); ok {
					metadata.CoverURL = "https://" + after
				}
			}
		}
	}
	return metadata, errors.Join(lyricErr, coverErr)
}

func (p *Provider) getJSON(ctx context.Context, endpoint string, target any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("Referer", "https://www.kuwo.cn/")
	resp, err := p.client.Do(req)
	if err != nil {
		return fmt.Errorf("Kuwo request: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("Kuwo returned HTTP %d", resp.StatusCode)
	}
	if err := json.NewDecoder(resp.Body).Decode(target); err != nil {
		return fmt.Errorf("decode Kuwo response: %w", err)
	}
	return nil
}

func parseQualities(info, formats string) []model.Quality {
	// N_MINFO 的内容并不完全稳定，同时结合 FORMAT 字段补充无损格式。
	seen := make(map[string]bool)
	result := make([]model.Quality, 0, 5)
	add := func(id string) {
		if quality, ok := qualities[id]; ok && !seen[id] {
			seen[id] = true
			result = append(result, quality)
		}
	}
	for _, part := range strings.FieldsFunc(info, func(r rune) bool { return r == ';' || r == ',' }) {
		pair := strings.SplitN(part, ":", 2)
		if len(pair) != 2 || !strings.EqualFold(strings.TrimSpace(pair[0]), "bitrate") {
			continue
		}
		switch strings.TrimSpace(pair[1]) {
		case "128":
			add("128kmp3")
		case "192":
			add("192kmp3")
		case "320":
			add("320kmp3")
		case "1000":
			add("1000kape")
		case "2000":
			add("2000kflac")
		}
	}
	if strings.Contains(strings.ToLower(formats), "flac") {
		add("2000kflac")
	}
	if strings.Contains(strings.ToLower(formats), "ape") {
		add("1000kape")
	}
	return result
}

func splitNonEmpty(value, separator string) []string {
	parts := strings.Split(value, separator)
	result := make([]string, 0, len(parts))
	for _, part := range parts {
		if part = strings.TrimSpace(part); part != "" {
			result = append(result, part)
		}
	}
	return result
}

func durationMillis(value string) int64 {
	seconds, _ := strconv.ParseFloat(strings.TrimSpace(value), 64)
	return int64(seconds * 1000)
}

func coverURL(path string) string {
	// 搜索接口有时只返回相对封面路径，需要补全酷我图片域名。
	path = strings.TrimSpace(path)
	if path == "" {
		return ""
	}
	if strings.HasPrefix(path, "http://") || strings.HasPrefix(path, "https://") {
		return path
	}
	path = strings.TrimPrefix(path, "/")
	path = strings.TrimPrefix(path, "120/")
	return "https://img3.kuwo.cn/star/albumcover/500/" + path
}

func flexibleInt(value any) int {
	switch item := value.(type) {
	case float64:
		return int(item)
	case string:
		result, _ := strconv.Atoi(item)
		return result
	default:
		return 0
	}
}
