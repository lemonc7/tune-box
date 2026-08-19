use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Song {
    pub id: i64,
    pub name: String,
    pub artists: Vec<String>,
    pub size: i64,
}

#[derive(Debug, Clone)]
pub struct SongPage {
    pub songs: Vec<Song>,
    pub next_before_id: Option<i64>,
}
