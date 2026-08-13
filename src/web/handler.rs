use axum::{
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue},
    response::{IntoResponse, Response},
};

use crate::{
    error::AppError,
    shared::{
        music::SearchResult,
        request::{CreateDownloadRequest, SearchRequest},
    },
    web::{
        extractor::{ValidateForm, ValidateQuery},
        pages::{
            HtmlTemplate, NoticeTemplate, SearchPageTemplate, SearchResultsTemplate,
            TrackListTemplate,
        },
        state::AppState,
    },
};

pub async fn health_check() -> &'static str {
    "Welcome to Tune-Box"
}

pub async fn home(State(s): State<AppState>) -> HtmlTemplate<SearchPageTemplate> {
    HtmlTemplate(SearchPageTemplate {
        providers: s.service.providers(),
    })
}

pub async fn search(
    State(s): State<AppState>,
    headers: HeaderMap,
    ValidateQuery(req): ValidateQuery<SearchRequest>,
) -> Result<Response, AppError> {
    let lossless = req.lossless;
    let result = s.service.search(req).await?;

    let next_url = next_search_url(&result, lossless);
    let load_more = headers
        .get("HX-Trigger")
        .and_then(|value| value.to_str().ok())
        == Some("search-more");

    if load_more {
        return Ok(HtmlTemplate(TrackListTemplate {
            result,
            lossless,
            next_url,
        })
        .into_response());
    }

    Ok(HtmlTemplate(SearchResultsTemplate {
        result,
        lossless,
        next_url,
    })
    .into_response())
}

pub async fn create_download(
    State(state): State<AppState>,
    ValidateForm(req): ValidateForm<CreateDownloadRequest>,
) -> Result<Response, AppError> {
    let artists = req
        .artists
        .into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|artist| !artist.is_empty())
        .collect::<Vec<_>>();

    if artists.is_empty() {
        return Err(AppError::BadRequest("歌曲歌手不能为空".to_owned()));
    }

    let request = CreateDownloadRequest {
        provider: req.provider,
        track_id: req.track_id,
        quality: req.quality,
        name: req.name.trim().to_owned(),
        artists,
        album: req.album.trim().to_owned(),
        album_artist: req.album_artist.trim().to_owned(),
        date: req.date.trim().to_owned(),
        cover_url: req.cover_url.trim().to_owned(),
    };

    let created = state.service.add_download(request).await?;

    let message = if created.is_some() {
        state.manager.wake().await?;
        "已加入下载队列"
    } else {
        "下载任务已存在"
    };

    let mut response = HtmlTemplate(NoticeTemplate {
        message: message.to_owned(),
    })
    .into_response();

    response.headers_mut().insert(
        HeaderName::from_static("hx-trigger"),
        HeaderValue::from_static("downloadsChanged"),
    );

    Ok(response)
}

fn next_search_url(result: &SearchResult, lossless: bool) -> Option<String> {
    let loaded = result.page * result.limit;
    if loaded >= result.total as i64 {
        return None;
    }

    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("provider", &result.provider)
        .append_pair("q", &result.query)
        .append_pair("page", &(result.page + 1).to_string())
        .append_pair("limit", &result.limit.to_string())
        .append_pair("lossless", if lossless { "true" } else { "false" })
        .finish();
    Some(format!("/search?{query}"))
}
