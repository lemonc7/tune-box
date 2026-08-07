package model

// Response 是 REST API 的统一响应结构。
type Response struct {
	Code int    `json:"code"`
	Msg  string `json:"msg"`
	Data any    `json:"data"`
}
