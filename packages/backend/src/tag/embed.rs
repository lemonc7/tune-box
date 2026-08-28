use std::{
    fs::File,
    io::{self, BufReader, Cursor},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
    time::Duration,
};

use contracts::music::Metadata;
use futures_util::StreamExt;
use lofty::{
    config::ParseOptions,
    file::AudioFile,
    flac::FlacFile,
    ogg::{OggPictureStorage, tag::VorbisComments},
    picture::{MimeType, Picture, PictureInformation, PictureType},
};
use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
    redirect,
};

use crate::{error::TagError, tag::flac_writer};

const MAX_COVER_BYTES: usize = 8 * 1024 * 1024;

pub struct Embedder {
    client: reqwest::Client,
    dns_timeout: Duration,
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
            .redirect(redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("封面图重定向次数过多")
                } else if has_safe_url_destination(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .no_proxy()
            .dns_resolver(PublicDnsResolver {
                timeout: dns_timeout,
            })
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
        let cover_info = self.fetch_cover(&metadata.cover_url).await?;
        let path = path.as_ref().to_owned();

        // FLAC 解析和文件重写是同步阻塞操作，不能直接占用 Tokio worker。
        tokio::task::spawn_blocking(move || {
            Self::write_flac_tags(&path, metadata, lyrics, cover_info)
        })
        .await?
    }

    fn write_flac_tags(
        path: &Path,
        metadata: Metadata,
        lyrics: String,
        cover_info: Option<Vec<u8>>,
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

        // metadata是完整快照：先清理旧的正面封面；其他类型图片保持不变。
        pictures.retain(|(picture, _)| picture.pic_type() != PictureType::CoverFront);
        if let Some(data) = cover_info {
            // 根据图片签名识别实际格式，避免仅信任 HTTP Content-Type。
            let mut picture = Picture::from_reader(&mut Cursor::new(data))
                .map_err(|_| TagError::UnsupportedCoverFormat)?;
            if !matches!(picture.mime_type(), Some(MimeType::Jpeg | MimeType::Png)) {
                return Err(TagError::UnsupportedCoverFormat);
            }
            picture.set_pic_type(PictureType::CoverFront);
            let information = PictureInformation::from_picture(&picture)
                .map_err(|_| TagError::UnsupportedCoverFormat)?;
            pictures.push((picture, information));
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
        validate_public_url(&url, self.dns_timeout).await?;

        let resp = self.client.get(url.as_str()).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(TagError::HttpStatus(status));
        }

        let content_length = resp.content_length();
        if content_length.is_some_and(|length| length > MAX_COVER_BYTES as u64) {
            return Err(TagError::CoverSizeInvalid);
        }

        // Content-Length 已经过上限校验；预分配可避免正常封面下载时多次扩容。
        // 响应没有长度时仍从空 Vec 开始，并由下面的累计大小检查兜底。
        let mut data = Vec::with_capacity(content_length.unwrap_or(0) as usize);
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

#[derive(Clone, Copy)]
struct PublicDnsResolver {
    timeout: Duration,
}

impl Resolve for PublicDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let timeout = self.timeout;
        Box::pin(async move {
            let addresses =
                tokio::time::timeout(timeout, tokio::net::lookup_host((host.as_str(), 0)))
                    .await
                    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "解析封面图域名超时"))??;
            let addresses = addresses
                .filter(|address| is_public_ip(address.ip()))
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "目标域名没有可用的公网地址",
                )
                .into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

async fn validate_public_url(url: &Url, timeout: Duration) -> Result<(), TagError> {
    if !has_safe_url_destination(url) {
        return Err(TagError::InvalidCoverUrl);
    }

    let port = url
        .port_or_known_default()
        .ok_or(TagError::InvalidCoverUrl)?;
    match url.host().ok_or(TagError::InvalidCoverUrl)? {
        url::Host::Ipv4(address) => {
            if !is_public_ip(IpAddr::V4(address)) {
                return Err(TagError::InvalidCoverUrl);
            }
        }
        url::Host::Ipv6(address) => {
            if !is_public_ip(IpAddr::V6(address)) {
                return Err(TagError::InvalidCoverUrl);
            }
        }
        url::Host::Domain(host) => {
            let addresses = tokio::time::timeout(timeout, tokio::net::lookup_host((host, port)))
                .await
                .map_err(|_| {
                    TagError::ResolveCoverHost(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "解析封面图域名超时",
                    ))
                })?
                .map_err(TagError::ResolveCoverHost)?
                .collect::<Vec<SocketAddr>>();
            if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
                return Err(TagError::InvalidCoverUrl);
            }
        }
    }
    Ok(())
}

fn has_safe_url_destination(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }

    match url.host() {
        Some(url::Host::Ipv4(address)) => is_public_ip(IpAddr::V4(address)),
        Some(url::Host::Ipv6(address)) => is_public_ip(IpAddr::V6(address)),
        Some(url::Host::Domain(host)) => !host.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = address.segments();
    // 只允许 2000::/3 全球单播，并排除文档地址 2001:db8::/32。
    (segments[0] & 0xe000) == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
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
    fn rejects_non_public_cover_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.168.0.1",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(address.parse().unwrap()), "{address}");
        }
        for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(address.parse().unwrap()), "{address}");
        }
    }

    #[tokio::test]
    async fn rejects_private_literal_cover_urls_before_requesting_them() {
        for value in [
            "http://127.0.0.1/cover.jpg",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/cover.jpg",
        ] {
            assert!(matches!(
                validate_public_url(&Url::parse(value).unwrap(), Duration::from_secs(1)).await,
                Err(TagError::InvalidCoverUrl)
            ));
        }
    }
}
