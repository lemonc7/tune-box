package web

import (
	"github.com/a-h/templ"
	"github.com/labstack/echo/v5"
)

func render(
	c *echo.Context,
	status int,
	component templ.Component,
) error {
	c.Response().Header().Set(
		"Content-Type",
		"text/html; charset=utf-8",
	)
	c.Response().WriteHeader(status)

	return component.Render(
		c.Request().Context(),
		c.Response(),
	)
}

// func renderError(c *echo.Context, err error) error {
// 	// htmx 默认不会交换 4xx 响应，所以参考版本使用 200，
// 	// 确保错误组件能够进入 hx-target。
// 	return render(c, http.StatusOK, components.ErrorNotice(err.Error()))
// }
