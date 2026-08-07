package model

// SearchRequest 是 REST 搜索接口的查询参数。
type SearchRequest struct {
	Provider string `query:"provider" validate:"required"`
	Query    string `query:"q" validate:"required"`
	Page     int    `query:"page" validate:"required,gte=1"`
	Limit    int    `query:"limit" validate:"required,gte=1,lte=100"`
	Lossless bool   `query:"lossless"`
}

// ListRequest 是下载历史和本地曲库的分页数量参数。
type ListRequest struct {
	Limit  int `query:"limit" validate:"required,gte=1,lte=100"`
	Offset int `query:"offset" validate:"gte=0"`
}

// ResolveRequest 指定需要解析的音乐源、歌曲和音质。
type ResolveRequest struct {
	Provider string `json:"provider" validate:"required"`
	TrackID  string `json:"trackId" validate:"required"`
	Quality  string `json:"quality" validate:"required"`
}

// DownloadRequest 在解析请求基础上增加可选文件名。
type DownloadRequest struct {
	ResolveRequest
	Metadata MusicMetadata `json:"metadata"`
}

type SearchSongsRequest struct {
	Keyword string `query:"keyword" validate:"required"`
	Limit   int    `query:"limit" validate:"required,gte=1"`
}
