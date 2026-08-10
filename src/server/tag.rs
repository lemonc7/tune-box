use std::path::Path;

use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{Picture, PictureType},
    probe::Probe,
    tag::{ItemKey, Tag},
};
use reqwest::Url;

use crate::{error::TagError, model::Metadata};

const MAX_COVER_BYTES: usize = 2 * 1024 * 1024;

pub struct Embedder {
    client: reqwest::Client,
}

#[derive(Debug, Clone, Default)]
pub struct FlacInfo {
    pub title: String,
    pub artists: Vec<String>,
}

impl Embedder {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
    pub async fn embed_flac(
        &self,
        path: impl AsRef<Path>,
        metadata: &Metadata,
        lyrics: &str,
    ) -> Result<(), TagError> {
        let path = path.as_ref();

        let cover_info = self.fetch_cover(&metadata.cover_url).await?;
        let mut tagged_file = Probe::open(path)?.read()?;
        // 获取或新建 primary tag
        let tag = match tagged_file.primary_tag_mut() {
            Some(t) => t,
            None => {
                let tag_type = tagged_file.primary_tag_type();
                tagged_file.insert_tag(Tag::new(tag_type));
                tagged_file.primary_tag_mut().unwrap()
            }
        };

        // 写入常见元数据字段
        Self::set_tag_field(tag, ItemKey::TrackTitle, &metadata.name);
        Self::set_tag_field(tag, ItemKey::TrackArtist, &metadata.artists.join("; "));
        Self::set_tag_field(tag, ItemKey::AlbumTitle, &metadata.album);
        Self::set_tag_field(tag, ItemKey::Lyrics, lyrics);
        Self::set_tag_field(tag, ItemKey::AlbumArtist, &metadata.album_artist);
        Self::set_tag_field(tag, ItemKey::RecordingDate, &metadata.date);

        // 写入封面
        if let Some(data) = cover_info {
            // 移除旧封面
            tag.remove_picture_type(PictureType::CoverFront);
            let mut picture = Picture::unchecked(data).build();
            picture.set_pic_type(PictureType::CoverFront);
            tag.push_picture(picture);
        }

        tagged_file.save_to_path(path, WriteOptions::default())?;
        Ok(())
    }

    fn set_tag_field(tag: &mut Tag, key: ItemKey, value: &str) {
        let value = value.trim();
        if !value.is_empty() {
            tag.insert_text(key, value.to_string());
        }
    }

    async fn fetch_cover(&self, cover_url: &str) -> Result<Option<Vec<u8>>, TagError> {
        let cover_url = cover_url.trim();
        if cover_url.is_empty() {
            return Ok(None);
        }

        let url = Url::parse(cover_url).map_err(|_| TagError::InvalidCoverUrl)?;
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(TagError::InvalidCoverUrl);
        }

        let resp = self.client.get(url.as_str()).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(TagError::HttpStatus(status));
        }

        let bytes = resp.bytes().await?;
        if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES {
            return Err(TagError::CoverSizeInvalid);
        }

        Ok(Some(bytes.to_vec()))
    }

    pub fn read_flac_metadata(path: impl AsRef<Path>) -> Result<FlacInfo, TagError> {
        let tagged_file = Probe::open(path)?.read()?;
        let mut info = FlacInfo::default();
        if let Some(tag) = tagged_file.primary_tag() {
            if let Some(title) = tag.get_string(ItemKey::TrackTitle) {
                info.title = title.trim().to_string();
            }
            if let Some(artist) = tag.get_string(ItemKey::TrackArtist) {
                info.artists = artist
                    .split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
        };

        Ok(info)
    }
}
