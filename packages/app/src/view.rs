use contracts::download::Status;
use dioxus::prelude::ServerFnError;

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

pub fn status_presentation(status: Status) -> (&'static str, &'static str) {
    match status {
        Status::Pending => ("等待中", "bg-muted text-muted-foreground"),
        Status::Resolving => ("解析资源", "bg-info/10 text-info"),
        Status::Downloading => ("下载中", "bg-primary/10 text-primary"),
        Status::Downloaded => ("等待标签", "bg-processing/10 text-processing"),
        Status::Tagging => ("写入标签", "bg-processing/10 text-processing"),
        Status::Paused => ("已暂停", "bg-warning/10 text-warning"),
        Status::Completed => ("已完成", "bg-success/10 text-success"),
        Status::Failed => ("失败", "bg-destructive/10 text-destructive"),
        Status::Cancelled => ("已取消", "bg-muted text-muted-foreground"),
    }
}

pub fn preview_url(provider: &str, track_id: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("provider", provider)
        .append_pair("track_id", track_id)
        .finish();
    format!("/api/preview?{query}")
}
