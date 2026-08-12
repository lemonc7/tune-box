use std::collections::HashMap;

pub const DOWNLOAD_DIR: &str = "downloads";

#[derive(Debug, Clone, Copy)]
pub struct QualitySpec {
    pub id: &'static str,
    pub label: &'static str,
    pub format: &'static str,
    pub bitrate: i32,
    pub lossless: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedResource {
    pub provider: &'static str,
    pub track_id: String,
    pub quality: QualitySpec,
    pub url: String,
    pub headers: HashMap<String, String>,
}

impl From<QualitySpec> for crate::shared::music::Quality {
    fn from(value: QualitySpec) -> Self {
        Self {
            id: value.id.to_string(),
            label: value.label.to_string(),
            format: value.format.to_string(),
            bitrate: value.bitrate,
            lossless: value.lossless,
        }
    }
}
