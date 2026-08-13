use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
};

use crate::shared::{
    download::{Download, DownloadPage, DownloadStatus},
    library::{Song, SongPage},
    music::SearchResult,
};

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

// 错误组件
#[derive(Template)]
#[template(path = "components/error.html")]
pub struct ErrorTemplate {
    pub message: String,
}

// 提示组件
#[derive(Template)]
#[template(path = "components/notice.html")]
pub struct NoticeTemplate {
    pub message: String,
}

// 搜索页面
#[derive(Template)]
#[template(path = "pages/search.html")]
pub struct SearchPageTemplate {
    pub providers: Vec<String>,
}

// 搜索结果(局部内容)
#[derive(Template)]
#[template(path = "components/search_results.html")]
pub struct SearchResultsTemplate {
    pub result: SearchResult,
    pub next_url: Option<String>,
}

// 搜索结果渲染组件
#[derive(Template)]
#[template(path = "components/track_list.html")]
pub struct TrackListTemplate {
    pub result: SearchResult,
    pub next_url: Option<String>,
}

// 下载历史页
#[derive(Template)]
#[template(path = "pages/downloads.html")]
pub struct DownloadsPageTemplate {
    pub page: DownloadPageView,
}

// 下载历史列表(局部内容)
#[derive(Template)]
#[template(path = "components/download_list.html")]
pub struct DownloadListTemplate {
    pub page: DownloadPageView,
}

// 单个下载任务的可刷新内容
#[derive(Template)]
#[template(path = "components/download_card.html")]
pub struct DownloadCardTemplate {
    pub download: DownloadView,
}

#[derive(Template)]
#[template(path = "pages/library.html")]
pub struct LibraryPageTemplate {
    pub query: String,
    pub count: i64,
    pub page: SongPageView,
}

#[derive(Template)]
#[template(path = "components/library_rows.html")]
pub struct LibraryRowsTemplate {
    pub page: SongPageView,
}

pub struct SongPageView {
    pub songs: Vec<SongView>,
    pub next_url: Option<String>,
}

pub struct SongView {
    pub id: i64,
    pub name: String,
    pub artists: String,
    pub extension: String,
    pub extension_class: &'static str,
    pub size: String,
}

pub struct DownloadPageView {
    pub downloads: Vec<DownloadView>,
    pub next_before_id: Option<i64>,
}

pub struct DownloadView {
    pub id: i64,
    pub name: String,
    pub artists: String,
    pub details: String,
    pub status_label: &'static str,
    pub status_class: &'static str,
    pub downloaded_text: String,
    pub progress_text: String,
    pub progress_percent: u64,
    pub polling: bool,
    pub has_actions: bool,
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_cancel: bool,
    pub can_retry: bool,
    pub can_delete: bool,
    pub error_message: String,
    pub has_error: bool,
}

impl SongPageView {
    pub fn new(page: SongPage, query: &str, limit: i64) -> Self {
        let next_url = page.next_before_id.map(|id| {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("q", query)
                .append_pair("before_id", &id.to_string())
                .append_pair("limit", &limit.to_string())
                .finish();
            format!("/library/items?{query}")
        });

        Self {
            songs: page.songs.into_iter().map(Into::into).collect(),
            next_url,
        }
    }
}

impl From<Song> for SongView {
    fn from(song: Song) -> Self {
        let extension_class = match song.extension.as_str() {
            "flac" => "bg-indigo-100 text-indigo-700",
            "mp3" => "bg-emerald-100 text-emerald-700",
            "wav" => "bg-cyan-100 text-cyan-700",
            "m4a" | "aac" => "bg-amber-100 text-amber-700",
            "ape" => "bg-violet-100 text-violet-700",
            "ogg" | "opus" => "bg-rose-100 text-rose-700",
            _ => "bg-slate-100 text-slate-600",
        };

        Self {
            id: song.id,
            name: song.name,
            artists: song.artists.join(" / "),
            extension: song.extension,
            extension_class,
            size: format_bytes(song.size),
        }
    }
}

impl From<DownloadPage> for DownloadPageView {
    fn from(page: DownloadPage) -> Self {
        Self {
            downloads: page.downloads.into_iter().map(Into::into).collect(),
            next_before_id: page.next_before_id,
        }
    }
}

impl From<Download> for DownloadView {
    fn from(download: Download) -> Self {
        let metadata = download.metadata;
        let progress_percent = download
            .total_bytes
            .filter(|total| *total > 0)
            .map(|total| {
                let downloaded = download.downloaded_bytes.max(0) as u64;
                let total = total as u64;
                downloaded
                    .saturating_mul(100)
                    .checked_div(total)
                    .unwrap_or(0)
                    .min(100)
            })
            .unwrap_or(0);
        let polling = matches!(
            download.status,
            DownloadStatus::Pending
                | DownloadStatus::Resolving
                | DownloadStatus::Downloading
                | DownloadStatus::Downloaded
                | DownloadStatus::Tagging
        );
        let can_pause = matches!(
            download.status,
            DownloadStatus::Pending | DownloadStatus::Resolving | DownloadStatus::Downloading
        );
        let can_resume = download.status == DownloadStatus::Paused;
        let can_cancel = !matches!(
            download.status,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        );
        let can_retry = matches!(
            download.status,
            DownloadStatus::Failed | DownloadStatus::Cancelled
        );
        let can_delete = matches!(
            download.status,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        );
        let (status_label, status_class) = status_presentation(download.status);
        let total_text = download
            .total_bytes
            .filter(|total| *total > 0)
            .map(format_bytes)
            .map(|total| format!("{progress_percent}% · {total}"))
            .unwrap_or_else(|| "总大小未知".to_owned());
        let has_error = download.error_message.is_some();
        let has_actions = can_pause || can_resume || can_cancel || can_retry || can_delete;
        let details = if metadata.album.is_empty() {
            format!("{} · {}", download.provider, download.quality)
        } else {
            format!(
                "{} · {} · {}",
                download.provider, download.quality, metadata.album
            )
        };

        Self {
            id: download.id,
            name: metadata.name,
            artists: metadata.artists.join(" / "),
            details,
            status_label,
            status_class,
            downloaded_text: format_bytes(download.downloaded_bytes),
            progress_text: total_text,
            progress_percent,
            polling,
            has_actions,
            can_pause,
            can_resume,
            can_cancel,
            can_retry,
            can_delete,
            error_message: download.error_message.unwrap_or_default(),
            has_error,
        }
    }
}

fn status_presentation(status: DownloadStatus) -> (&'static str, &'static str) {
    match status {
        DownloadStatus::Pending => ("等待中", "bg-slate-100 text-slate-600"),
        DownloadStatus::Resolving => ("解析资源", "bg-blue-50 text-blue-600"),
        DownloadStatus::Downloading => ("下载中", "bg-indigo-50 text-indigo-600"),
        DownloadStatus::Downloaded => ("等待标签", "bg-violet-50 text-violet-600"),
        DownloadStatus::Tagging => ("写入标签", "bg-violet-50 text-violet-600"),
        DownloadStatus::Paused => ("已暂停", "bg-amber-50 text-amber-600"),
        DownloadStatus::Completed => ("已完成", "bg-emerald-50 text-emerald-600"),
        DownloadStatus::Failed => ("失败", "bg-red-50 text-red-600"),
        DownloadStatus::Cancelled => ("已取消", "bg-slate-100 text-slate-500"),
    }
}

fn format_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}
