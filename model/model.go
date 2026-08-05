package model

// Quality 描述平台提供的一种音频质量。
type Quality struct {
	ID       string `json:"id"`
	Label    string `json:"label"`
	Format   string `json:"format"`
	Bitrate  int    `json:"bitrate"`
	Lossless bool   `json:"lossless"`
}

// Track 是不同音乐平台统一后的歌曲信息。
type Track struct {
	ID         string    `json:"id"`
	Provider   string    `json:"provider"`
	Name       string    `json:"name"`
	Artists    []string  `json:"artists"`
	ArtistIDs  []string  `json:"artistIds,omitempty"`
	Album      string    `json:"album,omitempty"`
	AlbumID    string    `json:"albumId,omitempty"`
	CoverURL   string    `json:"coverUrl,omitempty"`
	DurationMS int64     `json:"durationMs,omitempty"`
	Qualities  []Quality `json:"qualities"`
}

// SearchResult 是分页搜索结果。
type SearchResult struct {
	Provider string  `json:"provider"`
	Query    string  `json:"query"`
	Page     int     `json:"page"`
	Limit    int     `json:"limit"`
	Total    int     `json:"total"`
	Tracks   []Track `json:"tracks"`
}

// Resource 是 Provider 解析出的临时下载资源。
// Headers 保存下载该资源时必须携带的请求头。
type Resource struct {
	Provider  string            `json:"provider"`
	TrackID   string            `json:"trackId"`
	Quality   Quality           `json:"quality"`
	URL       string            `json:"url"`
	Headers   map[string]string `json:"headers,omitempty"`
	ExpiresAt string            `json:"expiresAt,omitempty"`
}


// DownloadResult 返回最终落盘文件的信息。
type DownloadResult struct {
	Path        string `json:"path"`
	FileName    string `json:"fileName"`
	Size        int64  `json:"size"`
	ContentType string `json:"contentType,omitempty"`
}
