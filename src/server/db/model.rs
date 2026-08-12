use serde::{Deserialize, Serialize};
use sqlx::{prelude::Type, types::Json};
use time::OffsetDateTime;

use crate::model::Metadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
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

#[derive(Debug, Clone)]
pub struct Download {
    pub id: i64,

    pub provider: String,
    pub track_id: String,
    pub quality: String,

    pub metadata: Json<Metadata>,
    pub status: Status,

    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,

    pub version: i64,
    pub retry_count: i64,
    pub next_retry_at: Option<OffsetDateTime>,

    pub path: Option<String>,
    pub error_message: Option<String>,

    pub created_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct Song {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size: i64,
    pub scan_token: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct SongArtist {
    pub id: i64,
    pub song_id: i64,
    pub artist: String,
}
