use serde::{Deserialize, Serialize};

use super::music::Metadata;

#[derive(Debug, Clone, Serialize)]
pub struct DownloadRequest {
    pub provider: String,
    pub track_id: String,
    pub quality: String,
    pub metadata: Metadata,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: i64,

    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub date: String,

    pub status: DownloadStatus,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub error_message: Option<String>,
}
