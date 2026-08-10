use std::collections::HashMap;

use serde::Serialize;

pub const DOWNLOAD_DIR: &str = "downloads";

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Quality {
    pub id: &'static str,
    pub label: &'static str,
    pub format: &'static str,
    pub bitrate: i32,
    pub lossless: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub id: String,
    pub provider: &'static str,
    pub name: String,
    pub artists: Vec<String>,
    pub artist_ids: Vec<String>,
    pub album: String,
    pub album_id: String,
    pub album_artist: String,
    pub date: String,
    pub cover_url: String,
    pub duration_ms: i64,
    pub qualities: Vec<Quality>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub provider: &'static str,
    pub query: String,
    pub page: u32,
    pub limit: u32,
    pub total: usize,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    pub provider: &'static str,
    pub track_id: String,
    pub quality: Quality,
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub date: String,
    pub cover_url: String,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub provider: String,
    pub track_id: String,
    pub quality: String,
    pub metadata: Metadata,
}
