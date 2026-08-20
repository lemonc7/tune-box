use serde::{Deserialize, Serialize};

use crate::music::Metadata;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Download {
    pub id: i64,
    pub provider: String,
    pub track_id: String,
    pub metadata: Metadata,
    pub status: Status,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadPage {
    pub downloads: Vec<Download>,
    pub next_before_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    Delete,
}
