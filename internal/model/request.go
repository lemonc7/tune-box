package model

import (
	"net/http"

	"github.com/go-playground/validator/v10"
	"github.com/labstack/echo/v5"
)

// SearchRequest 是 REST 搜索接口的查询参数。
type SearchRequest struct {
	Provider string `query:"provider" validate:"required"`
	Query    string `query:"q" validate:"required"`
	Page     int    `query:"page" validate:"required,gte=1"`
	Limit    int    `query:"limit" validate:"required,gte=1,lte=100"`
	Lossless bool   `query:"lossless"`
}

// ResolveRequest 指定需要解析的音乐源、歌曲和音质。
type ResolveRequest struct {
	Provider string `form:"provider" validate:"required"`
	TrackID  string `form:"trackId" validate:"required"`
	Quality  string `form:"quality" validate:"required"`
}

// DownloadRequest 在解析请求基础上增加可选文件名。
type DownloadRequest struct {
	ResolveRequest
	Metadata
}

type SearchSongsRequest struct {
	Keyword string `query:"q"`
	Offset  int    `query:"offset" validate:"gte=0"`
	Limit   int    `query:"limit" validate:"required,gte=1,lte=100"`
}

type Validator struct {
	V *validator.Validate
}

func (v *Validator) Validate(i any) error {
	if err := v.V.Struct(i); err != nil {
		return echo.NewHTTPError(http.StatusUnprocessableEntity, err.Error())
	}
	return nil
}
