package model

const DOWNLOAD_DIR = "downloads"

// Quality 描述平台提供的一种音频质量。
type Quality struct {
	ID       string
	Label    string
	Format   string
	Bitrate  int
	Lossless bool
}

// Track 是不同音乐平台统一后的歌曲信息。
type Track struct {
	ID          string
	Provider    string
	Name        string
	Artists     []string
	ArtistIDs   []string
	Album       string
	AlbumID     string
	AlbumArtist string
	Date        string
	CoverURL    string
	DurationMS  int64
	Qualities   []Quality
}

// SearchResult 是分页搜索结果。
type SearchResult struct {
	Provider string
	Query    string
	Page     int
	Limit    int
	Total    int
	Tracks   []Track
}

// Resource 是 Provider 解析出的临时下载资源。
type Resource struct {
	Provider  string
	TrackID   string
	Quality   Quality
	URL       string
	Headers   map[string]string
	ExpiresAt string
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
	Path        string
	FileName    string
	Size        int64
	ContentType string
}

// DownloadTaskResult 是创建或操作异步下载任务的结果。
type DownloadTaskResult struct {
	DownloadID int64
	Status     string
	Duplicate  bool
}

// DownloadHistory 描述一次下载从创建到结束的状态和结果。
type DownloadHistory struct {
	ID              int64
	Name            string
	Artists         []string
	Album           *string
	Provider        string
	TrackID         string
	Quality         string
	Status          string
	Progress        int64
	DownloadedBytes int64
	TotalBytes      int64
	Attempt         int64
	FileName        *string
	Path            *string
	ErrorMessage    *string
	CreatedAt       string
	StartedAt       *string
	FinishedAt      *string
}
