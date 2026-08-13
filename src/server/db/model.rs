use sqlx::types::{Json, time::OffsetDateTime};

use crate::shared::{
    download::{Download, DownloadStatus},
    music::Metadata,
};

#[derive(Debug, Clone)]
pub struct DownloadRow {
    pub id: i64,

    pub provider: String,
    pub track_id: String,
    pub quality: String,

    pub metadata: Json<Metadata>,
    pub status: DownloadStatus,

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

impl From<DownloadRow> for Download {
    fn from(value: DownloadRow) -> Self {
        let metadata = value.metadata.0;

        Self {
            id: value.id,
            name: metadata.name,
            artists: metadata.artists,
            album: metadata.album,
            date: metadata.date,
            status: value.status,
            downloaded_bytes: value.downloaded_bytes,
            total_bytes: value.total_bytes,
            error_message: value.error_message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SongRow {
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
pub struct SongArtistRow {
    pub id: i64,
    pub song_id: i64,
    pub artist: String,
}
