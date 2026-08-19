use serde::Serialize;

use crate::music::Metadata;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum Status {
    Pending,
    Resolving,
    Downloading,
    Downloaded,
    Tagging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct Download {
    pub id: i64,
    pub provider: String,
    pub track_id: String,
    pub metadata: Metadata,
    pub status: Status,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DownloadPage {
    pub downloads: Vec<Download>,
    pub next_before_id: Option<i64>,
}
