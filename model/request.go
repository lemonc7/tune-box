package model

type SearchRequest struct {
	Provider string `query:"provider" validate:"required"`
	Query    string `query:"q" validate:"required"`
	Page     int    `query:"page" validate:"required,gte=1"`
	Limit    int    `query:"limit" validate:"required,gte=10,lte=100"`
	Lossless bool   `query:"lossless"`
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
	FileName string `json:"fileName,omitempty"`
}
