use garde::Validate;
use serde::Deserialize;

fn non_blank(value: &str, _context: &()) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new("不能为空或只包含空白字符"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
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

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PreviewRequest {
    #[garde(custom(non_blank))]
    pub provider: String,
    #[garde(custom(non_blank))]
    pub track_id: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateDownloadRequest {
    #[garde(custom(non_blank))]
    pub provider: String,
    #[garde(custom(non_blank))]
    pub track_id: String,
    #[garde(custom(non_blank))]
    pub quality: String,
    #[garde(custom(non_blank))]
    pub name: String,
    #[garde(length(min = 1), inner(custom(non_blank)))]
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

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DownloadPageRequest {
    #[garde(range(min = 1))]
    pub before_id: i64,
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
}

fn default_library_page_size() -> i64 {
    20
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LibraryPageRequest {
    #[serde(rename = "q", default)]
    #[garde(skip)]
    pub query: String,
    #[serde(default)]
    #[garde(inner(range(min = 1)))]
    pub before_id: Option<i64>,
    #[serde(default = "default_library_page_size")]
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
}

#[cfg(test)]
mod tests {
    use garde::Validate;

    use super::CreateDownloadRequest;

    fn request() -> CreateDownloadRequest {
        CreateDownloadRequest {
            provider: "kuwo".to_owned(),
            track_id: "1".to_owned(),
            quality: "flac".to_owned(),
            name: "Song".to_owned(),
            artists: vec!["Artist".to_owned()],
            album: String::new(),
            album_artist: String::new(),
            date: String::new(),
            cover_url: String::new(),
        }
    }

    #[test]
    fn download_allows_empty_album_and_date() {
        assert!(request().validate().is_ok());
    }

    #[test]
    fn download_rejects_blank_required_fields_and_artists() {
        let mut value = request();
        value.name = "   ".to_owned();
        assert!(value.validate().is_err());

        let mut value = request();
        value.artists = vec!["   ".to_owned()];
        assert!(value.validate().is_err());
    }
}
