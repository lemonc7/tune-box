use garde::Validate;
use serde::Deserialize;

fn non_blank(value: &str, _context: &()) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new("不能为空或只包含空白字符"))
    } else {
        Ok(())
    }
}

fn optional_http_url(value: &str, _context: &()) -> garde::Result {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }

    let url = url::Url::parse(value).map_err(|_| garde::Error::new("必须是有效的 URL"))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(garde::Error::new("只允许 HTTP 或 HTTPS URL"))
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SearchRequest {
    #[garde(custom(non_blank), length(max = 64))]
    pub provider: String,
    #[garde(custom(non_blank), length(max = 200))]
    #[serde(rename = "q")]
    pub query: String,
    #[garde(range(min = 1))]
    pub page: i64,
    #[garde(range(min = 1, max = 100))]
    pub limit: i64,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PreviewRequest {
    #[garde(custom(non_blank), length(max = 64))]
    pub provider: String,
    #[garde(custom(non_blank), length(max = 256))]
    pub track_id: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateDownloadRequest {
    #[garde(custom(non_blank), length(max = 64))]
    pub provider: String,
    #[garde(custom(non_blank), length(max = 256))]
    pub track_id: String,
    #[garde(custom(non_blank), length(max = 32))]
    pub quality: String,
    #[garde(custom(non_blank), length(max = 300))]
    pub name: String,
    #[garde(length(min = 1, max = 20), inner(custom(non_blank), length(max = 200)))]
    #[serde(default)]
    pub artists: Vec<String>,
    #[garde(length(max = 300))]
    #[serde(default)]
    pub album: String,
    #[garde(length(max = 300))]
    #[serde(default)]
    pub album_artist: String,
    #[garde(length(max = 64))]
    #[serde(default)]
    pub date: String,
    #[garde(custom(optional_http_url), length(max = 2048))]
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

impl SearchRequest {
    pub fn normalize(mut self) -> Self {
        self.provider = self.provider.trim().to_owned();
        self.query = self.query.trim().to_owned();
        self
    }
}

impl CreateDownloadRequest {
    pub fn normalize(mut self) -> Self {
        self.provider = self.provider.trim().to_owned();
        self.track_id = self.track_id.trim().to_owned();
        self.quality = self.quality.trim().to_owned();
        self.name = self.name.trim().to_owned();
        self.artists = self
            .artists
            .into_iter()
            .map(|artist| artist.trim().to_owned())
            .filter(|artist| !artist.is_empty())
            .collect();
        self.album = self.album.trim().to_owned();
        self.album_artist = self.album_artist.trim().to_owned();
        self.date = self.date.trim().to_owned();
        self.cover_url = self.cover_url.trim().to_owned();
        self
    }
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

    #[test]
    fn download_normalization_and_limits_are_shared_by_all_entry_points() {
        let mut value = request();
        value.provider = " kuwo ".to_owned();
        value.name = " Song ".to_owned();
        value.artists = vec![" Alice ".to_owned(), "  ".to_owned()];
        let value = value.normalize();
        assert_eq!(value.provider, "kuwo");
        assert_eq!(value.name, "Song");
        assert_eq!(value.artists, ["Alice"]);
        assert!(value.validate().is_ok());

        let mut invalid_url = request();
        invalid_url.cover_url = "file:///etc/passwd".to_owned();
        assert!(invalid_url.validate().is_err());

        let mut too_many_artists = request();
        too_many_artists.artists = vec!["artist".to_owned(); 21];
        assert!(too_many_artists.validate().is_err());
    }
}
