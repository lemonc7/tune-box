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
pub struct MusicResult {
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
}
