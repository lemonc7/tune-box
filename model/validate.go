package model

import (
	"net/http"

	"github.com/go-playground/validator/v10"
	"github.com/labstack/echo/v5"
)

type Validator struct {
	V *validator.Validate
}

func (v *Validator) Validate(i any) error {
	if err := v.V.Struct(i); err != nil {
		return echo.NewHTTPError(http.StatusUnprocessableEntity, err.Error())
	}
	return nil
}
