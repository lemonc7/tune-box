use serde::{Deserialize, Serialize};

use crate::music::Metadata;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Song {
    pub id: i64,
    pub name: String,
    pub artists: Vec<String>,
    pub metadata: Option<Metadata>,
    pub extension: String,
    pub size: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SongPage {
    pub songs: Vec<Song>,
    pub next_before_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LibrarySnapshot {
    pub page: SongPage,
    pub count: i64,
}
