use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub date: String,
    pub cover_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub provider: String,
    pub query: String,
    pub page: i64,
    pub limit: i64,
    pub total: usize,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub date: String,
    pub cover_url: String,
    pub duration_ms: i64,
    pub qualities: Vec<Quality>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quality {
    pub id: String,
    pub label: String,
    pub format: String,
    pub bitrate: i32,
}
