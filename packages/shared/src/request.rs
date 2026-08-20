use garde::Validate;
use serde::{Deserialize, Serialize};

fn non_blank(value: &str, _context: &()) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new("不能为空或只包含空白字符"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate, PartialEq, Eq)]
pub struct SearchRequest {
    #[garde(custom(non_blank))]
    pub provider: String,
    #[garde(custom(non_blank))]
    #[serde(rename = "q")]
    pub query: String,
    #[garde(range(min = 1))]
    pub page: i64,
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate, PartialEq, Eq)]
pub struct PreviewRequest {
    #[garde(custom(non_blank))]
    pub provider: String,
    #[garde(custom(non_blank))]
    pub track_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate, PartialEq, Eq)]
pub struct CreateDownloadRequest {
    #[garde(custom(non_blank))]
    pub provider: String,
    #[garde(custom(non_blank))]
    pub track_id: String,
    #[garde(custom(non_blank))]
    pub name: String,
    #[garde(inner(custom(non_blank)))]
    #[serde(default)]
    pub artists: Vec<String>,
    #[garde(skip)]
    #[serde(default)]
    pub album: String,
    #[garde(skip)]
    #[serde(default)]
    pub album_artist: String,
    #[garde(skip)]
    #[serde(default)]
    pub date: String,
    #[garde(skip)]
    #[serde(default)]
    pub cover_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate, PartialEq, Eq)]
pub struct DownloadPageRequest {
    #[garde(range(min = 1))]
    pub before_id: i64,
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate, PartialEq, Eq)]
pub struct LibraryPageRequest {
    #[garde(skip)]
    #[serde(rename = "q", default)]
    pub query: String,
    #[garde(inner(range(min = 1)))]
    #[serde(default)]
    pub before_id: Option<i64>,
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
}
