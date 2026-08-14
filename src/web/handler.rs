use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};

use crate::{
    error::AppError,
    shared::{
        music::SearchResult,
        request::{
            CreateDownloadRequest, DownloadPageRequest, LibraryPageRequest, PreviewRequest,
            SearchRequest,
        },
    },
    web::{
        extractor::{ValidateForm, ValidateQuery},
        pages::{
            DownloadCardTemplate, DownloadListTemplate, DownloadsPageTemplate, HtmlTemplate,
            LibraryPageTemplate, LibraryRowsTemplate, NoticeTemplate, SearchPageTemplate,
            SearchResultsTemplate, SongPageView, TrackListTemplate,
        },
        state::AppState,
    },
};

const DOWNLOAD_PAGE_SIZE: i64 = 20;

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
    ValidateQuery(req): ValidateQuery<SearchRequest>,
) -> Result<HtmlTemplate<SearchResultsTemplate>, AppError> {
    let result = s.service.search(req).await?;
    let next_url = next_search_url(&result);

    Ok(HtmlTemplate(SearchResultsTemplate { result, next_url }))
}

pub async fn more_search_results(
    State(s): State<AppState>,
    ValidateQuery(req): ValidateQuery<SearchRequest>,
) -> Result<HtmlTemplate<TrackListTemplate>, AppError> {
    let result = s.service.search(req).await?;
    let next_url = next_search_url(&result);

    Ok(HtmlTemplate(TrackListTemplate { result, next_url }))
}

pub async fn stream_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidateQuery(req): ValidateQuery<PreviewRequest>,
) -> Result<Response, AppError> {
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let upstream = state
        .service
        .preview(&req.provider, &req.track_id, range)
        .await?;
    let status = upstream.status();
    if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
        tracing::warn!(%status, "试听资源服务器返回错误状态");
        return Ok((StatusCode::BAD_GATEWAY, "试听资源暂时不可用").into_response());
    }

    let upstream_headers = upstream.headers().clone();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::ETAG,
        header::LAST_MODIFIED,
        header::CACHE_CONTROL,
    ] {
        if let Some(value) = upstream_headers.get(&name) {
            response.headers_mut().insert(name, value.clone());
        }
    }
    Ok(response)
}

pub async fn create_download(
    State(state): State<AppState>,
    ValidateForm(req): ValidateForm<CreateDownloadRequest>,
) -> Result<Response, AppError> {
    let created = state.service.create_download(req).await?;

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

pub async fn downloads(
    State(s): State<AppState>,
) -> Result<HtmlTemplate<DownloadsPageTemplate>, AppError> {
    let page = s.service.list_downloads(None, DOWNLOAD_PAGE_SIZE).await?;

    Ok(HtmlTemplate(DownloadsPageTemplate { page: page.into() }))
}

pub async fn more_downloads(
    State(state): State<AppState>,
    ValidateQuery(req): ValidateQuery<DownloadPageRequest>,
) -> Result<HtmlTemplate<DownloadListTemplate>, AppError> {
    let page = state
        .service
        .list_downloads(Some(req.before_id), req.limit)
        .await?;

    Ok(HtmlTemplate(DownloadListTemplate { page: page.into() }))
}

pub async fn download_card(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<HtmlTemplate<DownloadCardTemplate>, AppError> {
    render_download_card(&state, id).await
}

pub async fn pause_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<HtmlTemplate<DownloadCardTemplate>, AppError> {
    if !state.manager.pause(id).await? {
        return Err(AppError::Conflict("当前任务不能暂停".to_owned()));
    }

    render_download_card(&state, id).await
}

pub async fn resume_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<HtmlTemplate<DownloadCardTemplate>, AppError> {
    if !state.manager.resume(id).await? {
        return Err(AppError::Conflict("当前任务不能继续".to_owned()));
    }

    render_download_card(&state, id).await
}

pub async fn cancel_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<HtmlTemplate<DownloadCardTemplate>, AppError> {
    if !state.manager.cancel(id).await? {
        return Err(AppError::Conflict("当前任务不能取消".to_owned()));
    }

    render_download_card(&state, id).await
}

pub async fn retry_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<HtmlTemplate<DownloadCardTemplate>, AppError> {
    if !state.manager.retry(id).await? {
        return Err(AppError::Conflict("当前任务不能重试".to_owned()));
    }
    state.manager.wake().await?;
    render_download_card(&state, id).await
}

pub async fn delete_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Html<&'static str>, AppError> {
    if !state.service.delete_download(id).await? {
        return Err(AppError::Conflict("当前任务不能删除".to_owned()));
    }
    Ok(Html(""))
}

pub async fn library(
    State(s): State<AppState>,
    ValidateQuery(req): ValidateQuery<LibraryPageRequest>,
) -> Result<HtmlTemplate<LibraryPageTemplate>, AppError> {
    let query = req.query.trim().to_owned();
    let page = s
        .service
        .list_songs(&query, req.before_id, req.limit)
        .await?;
    let count = s.service.count_songs().await?;
    Ok(HtmlTemplate(LibraryPageTemplate {
        page: SongPageView::new(page, &query, req.limit),
        query,
        count,
    }))
}

pub async fn library_items(
    State(s): State<AppState>,
    ValidateQuery(req): ValidateQuery<LibraryPageRequest>,
) -> Result<HtmlTemplate<LibraryRowsTemplate>, AppError> {
    let query = req.query.trim().to_owned();
    let page = s
        .service
        .list_songs(&query, req.before_id, req.limit)
        .await?;

    Ok(HtmlTemplate(LibraryRowsTemplate {
        page: SongPageView::new(page, &query, req.limit),
    }))
}

pub async fn sync_library(State(state): State<AppState>) -> Result<Response, AppError> {
    let count = state.service.sync_library().await?;
    let mut response = Html(format!("已同步 {count} 首歌曲")).into_response();
    response.headers_mut().insert(
        HeaderName::from_static("hx-trigger"),
        HeaderValue::from_static("librarySyncDone"),
    );
    Ok(response)
}

pub async fn delete_song(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    if !state.service.delete_song(id).await? {
        return Err(AppError::NotFound);
    }
    let mut response = Html("").into_response();
    response.headers_mut().insert(
        HeaderName::from_static("hx-trigger"),
        HeaderValue::from_static("downloadsChanged"),
    );
    Ok(response)
}

async fn render_download_card(
    state: &AppState,
    id: i64,
) -> Result<HtmlTemplate<DownloadCardTemplate>, AppError> {
    let download = state
        .service
        .get_download(id)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(HtmlTemplate(DownloadCardTemplate {
        download: download.into(),
    }))
}

fn next_search_url(result: &SearchResult) -> Option<String> {
    let loaded = result.page * result.limit;
    if loaded >= result.total as i64 {
        return None;
    }

    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("provider", &result.provider)
        .append_pair("q", &result.query)
        .append_pair("page", &(result.page + 1).to_string())
        .append_pair("limit", &result.limit.to_string())
        .finish();
    Some(format!("/search/more?{query}"))
}
