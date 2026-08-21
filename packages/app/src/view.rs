use dioxus::prelude::ServerFnError;
use shared::download::{Download, Status};

pub fn server_error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        error => error.to_string(),
    }
}

pub fn artists(artists: &[String]) -> String {
    if artists.is_empty() {
        "未知歌手".to_owned()
    } else {
        artists.join(" / ")
    }
}

pub fn format_bytes(bytes: i64) -> String {
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

pub fn progress_percent(download: &Download) -> u64 {
    download
        .total_bytes
        .filter(|total| *total > 0)
        .map(|total| {
            let downloaded = download.downloaded_bytes.max(0) as u64;
            downloaded
                .saturating_mul(100)
                .checked_div(total as u64)
                .unwrap_or_default()
                .min(100)
        })
        .unwrap_or_default()
}

pub fn status_presentation(status: Status) -> (&'static str, &'static str) {
    match status {
        Status::Pending => ("等待中", "bg-zinc-100 text-zinc-600"),
        Status::Resolving => ("解析资源", "bg-blue-50 text-blue-600"),
        Status::Downloading => ("下载中", "bg-indigo-50 text-indigo-600"),
        Status::Downloaded => ("等待标签", "bg-violet-50 text-violet-600"),
        Status::Tagging => ("写入标签", "bg-violet-50 text-violet-600"),
        Status::Paused => ("已暂停", "bg-amber-50 text-amber-600"),
        Status::Completed => ("已完成", "bg-emerald-50 text-emerald-600"),
        Status::Failed => ("失败", "bg-red-50 text-red-600"),
        Status::Cancelled => ("已取消", "bg-zinc-100 text-zinc-500"),
    }
}

pub fn extension_class(extension: &str) -> &'static str {
    match extension {
        "flac" => "bg-indigo-100 text-indigo-700",
        "mp3" => "bg-emerald-100 text-emerald-700",
        "wav" => "bg-cyan-100 text-cyan-700",
        "m4a" | "aac" => "bg-amber-100 text-amber-700",
        "ape" => "bg-violet-100 text-violet-700",
        "ogg" | "opus" => "bg-rose-100 text-rose-700",
        _ => "bg-zinc-100 text-zinc-600",
    }
}

pub fn preview_url(provider: &str, track_id: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("provider", provider)
        .append_pair("track_id", track_id)
        .finish();
    format!("/api/preview?{query}")
}
