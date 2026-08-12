use garde::Validate;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SearchRequest {
    #[garde(length(min = 1))]
    pub provider: String,
    #[garde(length(min = 1))]
    #[serde(rename = "q")]
    pub query: String,
    #[garde(range(min = 1))]
    pub page: i64,
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
    #[garde(skip)]
    pub lossless: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateDownloadRequest {
    #[garde(length(min = 1))]
    pub provider: String,
    #[garde(length(min = 1))]
    pub track_id: String,
    #[garde(length(min = 1))]
    pub quality: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1), inner(length(min = 1)))]
    pub artists: Vec<String>,
    #[garde(length(min = 1))]
    pub album: String,
    #[garde(length(min = 1))]
    pub album_artist: String,
    #[garde(length(min = 1))]
    pub date: String,
    #[garde(length(min = 1))]
    pub cover_url: String,
}
