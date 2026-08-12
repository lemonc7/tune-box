use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
};

use crate::shared::music::SearchResult;

pub struct HtmlTemplate<T>(pub T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> axum::response::Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(error) => {
                tracing::error!(
                  error = ?error,
                  "渲染 HTML 模板失败"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("<div class=\"text-red-600\">页面渲染失败<div/>"),
                )
                    .into_response()
            }
        }
    }
}

#[derive(Template)]
#[template(path = "fragments/error.html")]
pub struct ErrorTemplate {
    pub message: String,
}

#[derive(Template)]
#[template(path = "fragments/notice.html")]
pub struct NoticeTemplate {
    pub message: String,
}

#[derive(Template)]
#[template(path = "pages/search.html")]
pub struct SearchPageTemplate {
    pub providers: Vec<String>,
}

#[derive(Template)]
#[template(path = "fragments/search_results.html")]
pub struct SearchResultsTemplate {
    pub result: SearchResult,
    pub lossless: bool,
    pub next_url: Option<String>,
}

#[derive(Template)]
#[template(path = "fragments/track_list.html")]
pub struct TrackListTemplate {
    pub result: SearchResult,
    pub lossless: bool,
    pub next_url: Option<String>,
}
