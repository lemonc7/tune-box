use askama::Template;
use axum::{
    extract::{FromRequest, FromRequestParts, Query},
    http::{HeaderName, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
};
use axum_extra::extract::Form;
use serde::de::DeserializeOwned;

use crate::{
    error::{
        AppError::{self},
        ServiceError,
    },
    web::pages::ErrorTemplate,
};

pub struct ValidateQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ValidateQuery<T>
where
    T: DeserializeOwned + garde::Validate,
    T::Context: Default,
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|rejection| AppError::BadRequest(rejection.body_text()))?;

        value.validate()?;
        Ok(Self(value))
    }
}

pub struct ValidateForm<T>(pub T);

impl<S, T> FromRequest<S> for ValidateForm<T>
where
    T: DeserializeOwned + garde::Validate,
    T::Context: Default,
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let Form(value) = Form::<T>::from_request(req, state)
            .await
            .map_err(|error| AppError::BadRequest(error.body_text()))?;

        value.validate()?;
        Ok(Self(value))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::Validation(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "请检查输入内容".to_string(),
            ),
            AppError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            AppError::NotFound => (StatusCode::NOT_FOUND, "请求的内容不存在".to_owned()),
            AppError::Conflict(message) => (StatusCode::CONFLICT, message),
            AppError::Service(
                error @ (ServiceError::InvalidRequest(_)
                | ServiceError::InvalidProvider
                | ServiceError::UnsupportedDownloadQuality(_)),
            ) => (StatusCode::BAD_REQUEST, error.to_string()),
            AppError::Service(error) => {
                tracing::error!(
                  error = ?error,
                  "request failed"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "服务器内部错误，请稍后重试".to_owned(),
                )
            }
            AppError::Manager(error) => {
                tracing::error!(
                  error = ?error,
                  "manager request failed"
                );
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "下载任务管理器暂时不可用".to_owned(),
                )
            }
        };

        let body = match (ErrorTemplate { message }).render() {
            Ok(body) => body,
            Err(error) => {
                tracing::error!(
                  error = ?error,
                  "渲染错误模板失败"
                );
                "<div>服务器内部错误<div/>"
            }
            .to_owned(),
        };

        let mut resp = (status, Html(body)).into_response();
        resp.headers_mut().insert(
            HeaderName::from_static("hx-retarget"),
            HeaderValue::from_static("#app-error"),
        );

        resp.headers_mut().insert(
            HeaderName::from_static("hx-reswap"),
            HeaderValue::from_static("innerHTML"),
        );
        resp
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::FromRequest,
        http::{Request, header::CONTENT_TYPE},
    };
    use axum_extra::extract::Form;

    use crate::shared::request::CreateDownloadRequest;

    #[tokio::test]
    async fn form_collects_repeated_artist_fields() {
        let request = Request::builder()
            .method("POST")
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(
                "provider=kuwo&track_id=1&quality=flac&name=Song&artists=Alice&artists=Bob&album=Album&album_artist=Alice&date=2026&cover_url=https%3A%2F%2Fexample.com%2Fcover.jpg",
            ))
            .unwrap();

        let Form(value) = Form::<CreateDownloadRequest>::from_request(request, &())
            .await
            .unwrap();

        assert_eq!(value.artists, ["Alice", "Bob"]);
    }
}
