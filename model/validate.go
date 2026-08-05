package model

import (
	"net/http"

	"github.com/go-playground/validator/v10"
	"github.com/labstack/echo/v5"
)

// Validator 将 go-playground/validator 接入 Echo 的校验接口。
type Validator struct {
	V *validator.Validate
}

// Validate 使用 go-playground/validator 校验请求结构体。
func (v *Validator) Validate(i any) error {
	if err := v.V.Struct(i); err != nil {
		return echo.NewHTTPError(http.StatusUnprocessableEntity, err.Error())
	}
	return nil
}
