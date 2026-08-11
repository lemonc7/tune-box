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

impl Status {
    pub fn can_transition_to(self, next: Self) -> bool {
        self != next
            && matches!(
                (self, next),
                (Self::Pending, Self::Resolving)
                    | (Self::Resolving, Self::Downloading)
                    | (Self::Downloading, Self::Downloaded)
                    | (Self::Downloaded, Self::Tagging)
                    | (Self::Tagging, Self::Completed)
                    | (Self::Resolving | Self::Downloading, Self::Pending)
                    | (Self::Tagging, Self::Downloaded)
                    | (
                        Self::Pending | Self::Resolving | Self::Downloading,
                        Self::Paused,
                    )
                    | (Self::Paused, Self::Pending)
                    | (
                        Self::Pending
                            | Self::Resolving
                            | Self::Downloading
                            | Self::Downloaded
                            | Self::Tagging
                            | Self::Paused,
                        Self::Failed | Self::Cancelled
                    )
            )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
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
