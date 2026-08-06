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

// MusicMetadata 是写入音频文件的统一歌曲信息，歌词保留 LRC 时间轴。
type MusicMetadata struct {
	Name     string
	Artists  []string
	Album    string
	CoverURL string
	Lyrics   string
}

// DownloadResult 返回最终落盘文件的信息。
type DownloadResult struct {
	Path        string `json:"path"`
	FileName    string `json:"fileName"`
	Size        int64  `json:"size"`
	ContentType string `json:"contentType,omitempty"`
}

// DownloadTaskResult 是创建或操作异步下载任务的结果。
type DownloadTaskResult struct {
	DownloadID int64  `json:"downloadId"`
	Status     string `json:"status"`
	Duplicate  bool   `json:"duplicate,omitempty"`
}

// DownloadHistory 描述一次下载从创建到结束的状态和结果。
type DownloadHistory struct {
	ID              int64    `json:"id"`
	Name            string   `json:"name"`
	Artists         []string `json:"artists,omitempty"`
	Album           *string  `json:"album,omitempty"`
	Provider        string   `json:"provider"`
	TrackID         string   `json:"trackId"`
	Quality         string   `json:"quality"`
	Status          string   `json:"status"`
	Progress        int64    `json:"progress"`
	DownloadedBytes int64    `json:"downloadedBytes"`
	TotalBytes      int64    `json:"totalBytes"`
	Attempt         int64    `json:"attempt"`
	FileName        *string  `json:"fileName,omitempty"`
	Path            *string  `json:"path,omitempty"`
	Size            *int64   `json:"size,omitempty"`
	ContentType     *string  `json:"contentType,omitempty"`
	ErrorMessage    *string  `json:"errorMessage,omitempty"`
	CreatedAt       string   `json:"createdAt"`
	StartedAt       *string  `json:"startedAt,omitempty"`
	FinishedAt      *string  `json:"finishedAt,omitempty"`
}

// LibraryFile 是下载目录中实际存在的一份音频文件。
type LibraryFile struct {
	ID         int64   `json:"id"`
	DownloadID *int64  `json:"downloadId,omitempty"`
	Path       string  `json:"path"`
	FileName   string  `json:"fileName"`
	Extension  string  `json:"extension"`
	Size       int64   `json:"size"`
	ModifiedAt string  `json:"modifiedAt"`
	Provider   *string `json:"provider,omitempty"`
	TrackID    *string `json:"trackId,omitempty"`
	Quality    *string `json:"quality,omitempty"`
	Name       *string `json:"name,omitempty"`
	Album      *string `json:"album,omitempty"`
}

// LibraryScanResult 返回本次目录扫描发现的音频文件数量。
type LibraryScanResult struct {
	Files int `json:"files"`
}
