use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SongDto {
    pub id: i64,
    pub name: String,
    pub artists: Vec<String>,
    pub extension: String,
    pub size: i64,
    pub media_url: String,
}
