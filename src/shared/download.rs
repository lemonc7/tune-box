use super::music::Metadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum DownloadStatus {
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

#[derive(Debug, Clone)]
pub struct Download {
    pub id: i64,

    pub provider: String,
    pub track_id: String,
    pub quality: String,

    pub metadata: Metadata,
    pub status: DownloadStatus,

    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,

    pub version: i64,
    pub retry_count: i64,

    pub path: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadPage {
    pub downloads: Vec<Download>,
    pub next_before_id: Option<i64>,
}
