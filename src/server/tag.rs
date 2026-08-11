use std::{io::Cursor, path::Path};

use futures_util::StreamExt;
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    probe::Probe,
    tag::{ItemKey, ItemValue, Tag, TagItem},
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
        metadata: Metadata,
        lyrics: String,
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
        Self::set_tag_field(tag, ItemKey::TrackTitle, metadata.name);
        Self::set_artists(tag, &metadata.artists);
        Self::set_tag_field(tag, ItemKey::AlbumTitle, metadata.album);
        Self::set_tag_field(tag, ItemKey::Lyrics, lyrics);
        Self::set_tag_field(tag, ItemKey::AlbumArtist, metadata.album_artist);
        Self::set_tag_field(tag, ItemKey::RecordingDate, metadata.date);

        // metadata是完整快照：先清理旧的正面封面；其他类型图片保持不变。
        tag.remove_picture_type(PictureType::CoverFront);
        if let Some(data) = cover_info {
            // 根据图片签名识别实际格式，避免仅信任 HTTP Content-Type。
            let mut picture = Picture::from_reader(&mut Cursor::new(data))
                .map_err(|_| TagError::UnsupportedCoverFormat)?;
            if !matches!(picture.mime_type(), Some(MimeType::Jpeg | MimeType::Png)) {
                return Err(TagError::UnsupportedCoverFormat);
            }
            picture.set_pic_type(PictureType::CoverFront);
            tag.push_picture(picture);
        }

        tagged_file.save_to_path(path, WriteOptions::default())?;
        Ok(())
    }

    fn set_tag_field(tag: &mut Tag, key: ItemKey, value: String) {
        if value.trim().is_empty() {
            tag.remove_key(key);
            return;
        };

        let mut value = value;
        let end = value.trim_end().len();
        value.truncate(end);
        let start = value.len() - value.trim_start().len();
        if start > 0 {
            value.drain(..start);
        }

        tag.insert_text(key, value);
    }

    fn set_artists(tag: &mut Tag, artists: &[String]) {
        tag.remove_key(ItemKey::TrackArtist);

        for artist in artists {
            let artist = artist.trim();
            if artist.is_empty() {
                continue;
            }

            tag.push(TagItem::new(
                ItemKey::TrackArtist,
                ItemValue::Text(artist.to_string()),
            ));
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

        if resp
            .content_length()
            .is_some_and(|length| length > MAX_COVER_BYTES as u64)
        {
            return Err(TagError::CoverSizeInvalid);
        }

        let mut data = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if data.len() + chunk.len() > MAX_COVER_BYTES {
                return Err(TagError::CoverSizeInvalid);
            }
            data.extend_from_slice(&chunk);
        }
        if data.is_empty() {
            return Err(TagError::CoverSizeInvalid);
        }

        Ok(Some(data))
    }

    pub fn read_flac_metadata(path: impl AsRef<Path>) -> Result<FlacInfo, TagError> {
        let tagged_file = Probe::open(path)?.read()?;
        let mut info = FlacInfo::default();
        if let Some(tag) = tagged_file.primary_tag() {
            if let Some(title) = tag.get_string(ItemKey::TrackTitle) {
                info.title = title.trim().to_string();
            }
            info.artists = artists_from_tag(tag);
        };

        Ok(info)
    }
}

fn artists_from_tag(tag: &Tag) -> Vec<String> {
    tag.get_strings(ItemKey::TrackArtist)
        // 兼容旧版本写入的单个 "artist1; artist2" 字段。
        .flat_map(|artist| artist.split(';'))
        .map(str::trim)
        .filter(|artist| !artist.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::tag::TagType;

    #[test]
    fn set_tag_field_replaces_and_removes_values() {
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.insert_text(ItemKey::TrackTitle, "old title".to_string());

        Embedder::set_tag_field(&mut tag, ItemKey::TrackTitle, " new title ".to_string());
        assert_eq!(tag.get_string(ItemKey::TrackTitle), Some("new title"));

        Embedder::set_tag_field(&mut tag, ItemKey::TrackTitle, "  ".to_string());
        assert_eq!(tag.get_string(ItemKey::TrackTitle), None);
    }

    #[test]
    fn writes_artists_as_multiple_fields() {
        let mut tag = Tag::new(TagType::VorbisComments);
        let artists = vec![
            " artist one ".to_string(),
            "".to_string(),
            "artist two".to_string(),
        ];

        Embedder::set_artists(&mut tag, &artists);

        assert_eq!(
            tag.get_strings(ItemKey::TrackArtist).collect::<Vec<_>>(),
            ["artist one", "artist two"]
        );
    }

    #[test]
    fn reads_multiple_and_legacy_artist_fields() {
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.push(TagItem::new(
            ItemKey::TrackArtist,
            ItemValue::Text("artist one; artist two".to_string()),
        ));
        tag.push(TagItem::new(
            ItemKey::TrackArtist,
            ItemValue::Text("artist three".to_string()),
        ));

        assert_eq!(
            artists_from_tag(&tag),
            ["artist one", "artist two", "artist three"]
        );
    }
}
