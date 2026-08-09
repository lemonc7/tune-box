package model

const DOWNLOAD_DIR = "downloads"

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
	ID          string    `json:"id"`
	Provider    string    `json:"provider"`
	Name        string    `json:"name"`
	Artists     []string  `json:"artists"`
	ArtistIDs   []string  `json:"artistIds,omitempty"`
	Album       string    `json:"album"`
	AlbumID     string    `json:"albumId,omitempty"`
	AlbumArtist string    `json:"albumArtist"`
	Date        string    `json:"date"`
	CoverURL    string    `json:"coverUrl"`
	DurationMS  int64     `json:"durationMs"`
	Qualities   []Quality `json:"qualities"`
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
type Resource struct {
	Provider  string            `json:"provider"`
	TrackID   string            `json:"trackId"`
	Quality   Quality           `json:"quality"`
	URL       string            `json:"url"`
	Headers   map[string]string `json:"headers"`
	ExpiresAt string            `json:"expiresAt"`
}

// Metadata 是写入音频文件的统一歌曲信息，歌词保留 LRC 时间轴。
type Metadata struct {
	Name        string   `form:"name"`
	Artists     []string `form:"artists"`
	Album       string   `form:"album"`
	AlbumArtist string   `form:"albumArtist"`
	Date        string   `form:"date"`
	CoverURL    string   `form:"coverUrl"`
}

// DownloadResult 返回最终落盘文件的信息。
type DownloadResult struct {
	Path        string `json:"path"`
	FileName    string `json:"fileName"`
	Size        int64  `json:"size"`
	ContentType string `json:"contentType"`
}

// DownloadTaskResult 是创建或操作异步下载任务的结果。
type DownloadTaskResult struct {
	DownloadID int64  `json:"downloadId"`
	Status     string `json:"status"`
	Duplicate  bool   `json:"duplicate"`
}

// DownloadHistory 描述一次下载从创建到结束的状态和结果。
type DownloadHistory struct {
	ID              int64    `json:"id"`
	Name            string   `json:"name"`
	Artists         []string `json:"artists"`
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
	ErrorMessage    *string  `json:"errorMessage,omitempty"`
	CreatedAt       string   `json:"createdAt"`
	StartedAt       *string  `json:"startedAt,omitempty"`
	FinishedAt      *string  `json:"finishedAt,omitempty"`
}
