use std::{fs::File, io::BufReader, path::Path, time::Duration};

use contracts::music::Metadata;
use futures_util::StreamExt;
use lofty::{
    config::ParseOptions,
    file::AudioFile,
    flac::FlacFile,
    ogg::{OggPictureStorage, tag::VorbisComments},
    picture::{MimeType, Picture, PictureInformation, PictureType},
};
use reqwest::Url;

use crate::{
    error::TagError,
    net::{NetError, PublicDnsResolver, public_redirect_policy, validate_public_url},
    tag::flac_writer,
};

// 封面下载上限。内嵌封面通常只有几百 KB，4MB 足以容纳高分辨率图片。
// 这个值同时决定单个打标签任务的瞬时内存峰值，调大要按并发数重新估算。
const MAX_COVER_BYTES: usize = 4 * 1024 * 1024;
// 封面缓冲区的起始容量。
const INITIAL_COVER_CAPACITY: usize = 64 * 1024;

pub struct Embedder {
    client: reqwest::Client,
    dns_timeout: Duration,
}

enum CoverUpdate {
    Preserve,
    Remove,
    Replace(Vec<u8>),
}

#[derive(Debug, Clone, Default)]
pub struct FlacInfo {
    pub title: String,
    pub artists: Vec<String>,
}

impl Embedder {
    pub fn new(timeout: Duration) -> Result<Self, reqwest::Error> {
        let dns_timeout = timeout.min(Duration::from_secs(10));
        // 封面地址来自客户端元数据。禁用系统代理，并在真正建立连接的 DNS
        // 解析器中只返回公网地址，避免 DNS 重绑定绕过预检查。
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .read_timeout(timeout)
            .redirect(public_redirect_policy())
            .no_proxy()
            .dns_resolver(PublicDnsResolver::new(timeout))
            .build()?;
        Ok(Self {
            client,
            dns_timeout,
        })
    }

    pub async fn embed_flac(
        &self,
        path: impl AsRef<Path>,
        metadata: Metadata,
        lyrics: String,
    ) -> Result<(), TagError> {
        let cover_update = match self.fetch_cover(&metadata.cover_url).await {
            Ok(Some(data)) => CoverUpdate::Replace(data),
            Ok(None) => CoverUpdate::Remove,
            Err(error) => {
                // 封面属于可选增强信息。获取失败时保留音频已有封面，继续写文字标签。
                tracing::warn!(%error, "获取封面失败，保留原有封面继续写标签");
                CoverUpdate::Preserve
            }
        };
        let path = path.as_ref().to_owned();

        // FLAC 解析和文件重写是同步阻塞操作，不能直接占用 Tokio worker。
        tokio::task::spawn_blocking(move || {
            Self::write_flac_tags(&path, metadata, lyrics, cover_update)
        })
        .await?
    }

    fn write_flac_tags(
        path: &Path,
        metadata: Metadata,
        lyrics: String,
        cover_update: CoverUpdate,
    ) -> Result<(), TagError> {
        let mut reader = BufReader::new(File::open(path)?);
        // 这里只读取文件开头的元数据链。关闭音频属性计算可以避免为时长、码率等
        // 信息继续扫描音频帧；封面仍需读取，以便保留非正面封面的原有图片。
        let options = ParseOptions::new()
            .read_properties(false)
            .read_cover_art(true);
        let mut flac = FlacFile::read_from(&mut reader, options)?;
        let mut tag = flac.remove_vorbis_comments().unwrap_or_default();

        // FLAC 图片可能来自原生 PICTURE block，也可能来自 Vorbis Comment；统一转成原生块。
        let mut pictures = tag.remove_pictures();
        pictures.extend(flac.remove_pictures());
        // 后续只保留拆出的标签和图片；尽早释放解析器持有的其他元数据及文件句柄，
        // 再由流式写入器重新打开源文件。
        drop(flac);
        drop(reader);

        // 写入常见元数据字段
        Self::set_tag_field(&mut tag, "TITLE", metadata.name);
        Self::set_artists(&mut tag, &metadata.artists);
        Self::set_tag_field(&mut tag, "ALBUM", metadata.album);
        Self::set_tag_field(&mut tag, "LYRICS", lyrics);
        Self::set_tag_field(&mut tag, "ALBUMARTIST", metadata.album_artist);
        Self::set_date_fields(&mut tag, &metadata.date);
        Self::set_tag_field(&mut tag, "TRACKNUMBER", metadata.track_number);
        Self::set_tag_field(&mut tag, "DISCNUMBER", metadata.disc_number);

        match cover_update {
            CoverUpdate::Preserve => {}
            CoverUpdate::Remove => {
                pictures.retain(|(picture, _)| picture.pic_type() != PictureType::CoverFront);
            }
            CoverUpdate::Replace(data) => match front_cover(data) {
                Ok(cover) => {
                    pictures.retain(|(picture, _)| picture.pic_type() != PictureType::CoverFront);
                    pictures.push(cover);
                }
                Err(error) => {
                    // 图片内容损坏或格式不支持时同样降级，并保留音频原有封面。
                    tracing::warn!(%error, "解析封面失败，保留原有封面继续写标签");
                }
            },
        }

        flac_writer::rewrite_flac_tags(path, &tag, &pictures)
    }

    fn set_tag_field(tag: &mut VorbisComments, key: &str, value: String) {
        let value = value.trim();
        if value.is_empty() {
            tag.remove(key).for_each(drop);
        } else {
            tag.insert(key.to_string(), value.to_string());
        }
    }

    fn set_artists(tag: &mut VorbisComments, artists: &[String]) {
        tag.remove("ARTIST").for_each(drop);

        for artist in artists {
            let artist = artist.trim();
            if artist.is_empty() {
                continue;
            }

            tag.push("ARTIST".to_string(), artist.to_string());
        }
    }

    fn set_date_fields(tag: &mut VorbisComments, date: &str) {
        Self::set_tag_field(tag, "DATE", date.to_string());
        Self::set_tag_field(
            tag,
            "YEAR",
            release_year(date).unwrap_or_default().to_string(),
        );
    }

    async fn fetch_cover(&self, cover_url: &str) -> Result<Option<Vec<u8>>, TagError> {
        let cover_url = cover_url.trim();
        if cover_url.is_empty() {
            return Ok(None);
        }

        let url = Url::parse(cover_url).map_err(|_| TagError::InvalidCoverUrl)?;
        validate_public_url(&url, self.dns_timeout)
            .await
            .map_err(|error| match error {
                NetError::NotPublicAddress => TagError::InvalidCoverUrl,
                NetError::Resolve(error) => TagError::ResolveCoverHost(error),
            })?;

        let resp = self.client.get(url.as_str()).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(TagError::HttpStatus(status));
        }

        let content_length = resp.content_length();
        if content_length.is_some_and(|length| length > MAX_COVER_BYTES as u64) {
            return Err(TagError::CoverSizeInvalid);
        }

        // 起始容量固定，不按远端声明的 Content-Length 预分配：那个值由对端控制，
        // 会把单个任务的峰值直接顶到上限。正常封面只有几百 KB，扩容次数很少；
        // 响应超过上限时由下面的累计大小检查兜底截断。
        let mut data = Vec::with_capacity(INITIAL_COVER_CAPACITY);
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

    pub async fn read_flac_metadata(path: impl AsRef<Path>) -> Result<FlacInfo, TagError> {
        let path = path.as_ref().to_owned();
        tokio::task::spawn_blocking(move || Self::read_flac_metadata_blocking(&path)).await?
    }

    fn read_flac_metadata_blocking(path: &Path) -> Result<FlacInfo, TagError> {
        let mut reader = BufReader::new(File::open(path)?);
        // 曲库扫描只需要文字字段，不读取音频属性和封面，避免无意义的大对象分配。
        let options = ParseOptions::new()
            .read_properties(false)
            .read_cover_art(false);
        let flac = FlacFile::read_from(&mut reader, options)?;
        let mut info = FlacInfo::default();
        if let Some(tag) = flac.vorbis_comments() {
            if let Some(title) = tag.get("TITLE") {
                info.title = title.trim().to_string();
            }
            info.artists = artists_from_tag(tag);
        };

        Ok(info)
    }
}

fn front_cover(data: Vec<u8>) -> Result<(Picture, PictureInformation), TagError> {
    // 根据图片签名识别实际格式，避免仅信任 HTTP Content-Type。
    // 只接受 FLAC 常用的 PNG/JPEG，签名规则与 lofty 内部一致。
    let mime_type = match data.first_chunk::<8>() {
        Some([0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) => MimeType::Png,
        Some([0xFF, 0xD8, ..]) => MimeType::Jpeg,
        _ => return Err(TagError::UnsupportedCoverFormat),
    };

    // unchecked 直接接管 Vec。from_reader 内部会 read_to_end 再整块复制一份，
    // 峰值等于封面大小的两倍；这里只保留一份数据并原地转移所有权。
    let picture = Picture::unchecked(data)
        .mime_type(mime_type)
        .pic_type(PictureType::CoverFront)
        .build();
    let information =
        PictureInformation::from_picture(&picture).map_err(|_| TagError::UnsupportedCoverFormat)?;
    Ok((picture, information))
}

fn artists_from_tag(tag: &VorbisComments) -> Vec<String> {
    tag.get_all("ARTIST")
        .map(str::trim)
        .filter(|artist| !artist.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn release_year(date: &str) -> Option<&str> {
    let date = date.trim();
    let year = date.get(..4)?;
    if !year.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if date.len() == 4
        || date
            .as_bytes()
            .get(4)
            .is_some_and(|separator| matches!(separator, b'-' | b'/' | b'.'))
    {
        Some(year)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_tag_field_replaces_and_removes_values() {
        let mut tag = VorbisComments::new();
        tag.insert("TITLE".to_string(), "old title".to_string());

        Embedder::set_tag_field(&mut tag, "TITLE", " new title ".to_string());
        assert_eq!(tag.get("TITLE"), Some("new title"));

        Embedder::set_tag_field(&mut tag, "TITLE", "  ".to_string());
        assert_eq!(tag.get("TITLE"), None);
    }

    #[test]
    fn writes_artists_as_multiple_fields() {
        let mut tag = VorbisComments::new();
        let artists = vec![
            " artist one ".to_string(),
            "".to_string(),
            "artist two".to_string(),
        ];

        Embedder::set_artists(&mut tag, &artists);

        assert_eq!(
            tag.get_all("ARTIST").collect::<Vec<_>>(),
            ["artist one", "artist two"]
        );
    }

    #[test]
    fn writes_full_date_and_compatible_year() {
        let mut tag = VorbisComments::new();

        Embedder::set_date_fields(&mut tag, "2026-08-28");

        assert_eq!(tag.get("DATE"), Some("2026-08-28"));
        assert_eq!(tag.get("YEAR"), Some("2026"));
    }

    #[test]
    fn removes_stale_year_when_date_has_no_valid_year() {
        let mut tag = VorbisComments::new();
        tag.insert("YEAR".to_string(), "1999".to_string());

        Embedder::set_date_fields(&mut tag, "unknown");

        assert_eq!(tag.get("DATE"), Some("unknown"));
        assert_eq!(tag.get("YEAR"), None);
    }

    #[test]
    fn reads_each_artist_field_without_guessing_delimiters() {
        let mut tag = VorbisComments::new();
        tag.push("ARTIST".to_string(), "artist one; artist two".to_string());
        tag.push("ARTIST".to_string(), "artist three".to_string());

        assert_eq!(
            artists_from_tag(&tag),
            ["artist one; artist two", "artist three"]
        );
    }

    #[test]
    fn detects_cover_format_from_signature_and_rejects_everything_else() {
        let png = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        ];
        let (picture, _) = front_cover(png.to_vec()).expect("PNG 签名应被接受");
        assert_eq!(picture.mime_type(), Some(&MimeType::Png));
        assert_eq!(picture.pic_type(), PictureType::CoverFront);

        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        let (picture, _) = front_cover(jpeg.to_vec()).expect("JPEG 签名应被接受");
        assert_eq!(picture.mime_type(), Some(&MimeType::Jpeg));

        // 只接受 FLAC 常用的 PNG/JPEG；GIF 之类的格式即便能被识别也要拒绝。
        let gif = [b'G', b'I', b'F', b'8', b'9', b'a', 0x01, 0x02];
        assert!(matches!(
            front_cover(gif.to_vec()),
            Err(TagError::UnsupportedCoverFormat)
        ));

        // 不足 8 字节时无法判定签名
        assert!(front_cover(vec![0xFF, 0xD8, 0xFF]).is_err());
    }
}
