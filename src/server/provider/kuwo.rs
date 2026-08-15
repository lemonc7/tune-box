use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use reqwest::Url;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::server::provider::USER_AGENT;
use crate::{
    error::ProviderError,
    server::{
        model::{DOWNLOAD_QUALITY_ID, PREVIEW_QUALITY_ID, QualitySpec, ResolvedResource},
        provider::Provider,
    },
    shared::music::{Quality, SearchResult, Track as SearchTrack},
};

const COVER_URL: &str = "https://img3.kuwo.cn/star/albumcover/500/";
const ALBUM_INFO_CONCURRENCY: usize = 6;
const MAX_ALBUM_INFO_PER_SEARCH: usize = 30;
const ALBUM_CACHE_CAPACITY: usize = 1024;
const ALBUM_INFO_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const ALBUM_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const ALBUM_CACHE_ERROR_TTL: Duration = Duration::from_secs(5 * 60);

pub struct KuwoProvider {
    client: reqwest::Client,
    search_url: Url,
    resolve_url: Url,
    lyric_url: Url,
    album_cache: RwLock<HashMap<String, CachedAlbumInfo>>,
}

#[derive(Clone)]
struct CachedAlbumInfo {
    value: (String, String),
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(rename = "TOTAL", deserialize_with = "flexible_int")]
    total: usize,
    #[serde(rename = "abslist")]
    abs_list: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct ResolveResponse {
    data: ResolveData,
}

#[derive(Debug, Deserialize)]
struct ResolveData {
    url: String,
}

#[derive(Debug, Deserialize)]
struct LyricResponse {
    data: Lines,
}

#[derive(Debug, Deserialize)]
struct Lines {
    #[serde(rename = "lrclist")]
    lines: Vec<Line>,
}

#[derive(Debug, Deserialize)]
struct Line {
    #[serde(rename = "lineLyric")]
    text: String,
    time: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Track {
    #[serde(rename = "ALBUM")]
    album: String,
    #[serde(rename = "AARTIST")]
    album_artist: String,
    #[serde(rename = "ALBUMID")]
    album_id: String,
    #[serde(rename = "ARTIST")]
    artist: String,
    #[serde(rename = "DURATION")]
    duration: String,
    #[serde(rename = "MUSICRID")]
    music_rid: String,
    #[serde(rename = "NAME")]
    name: String,
    #[serde(rename = "N_MINFO")]
    music_info: String,
    #[serde(rename = "FORMAT")]
    format: String,
    #[serde(rename = "web_albumpic_short")]
    album_picture: String,
}

#[derive(Debug, Deserialize)]
struct AlbumInfo {
    #[serde(rename = "pub")]
    date: String,
    #[serde(rename = "artist")]
    artist: String,
}

impl KuwoProvider {
    pub fn new(
        client: reqwest::Client,
        search_url: String,
        resolve_url: String,
        lyric_url: String,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            client,
            search_url: parse_config_url(&search_url, "酷我搜索API")?,
            resolve_url: parse_config_url(&resolve_url, "酷我解析API")?,
            lyric_url: parse_config_url(&lyric_url, "酷我歌词API")?,
            album_cache: RwLock::new(HashMap::new()),
        })
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, ProviderError> {
        let resp = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://www.kuwo.cn/")
            .send()
            .await?
            .error_for_status()?;
        let data = resp.json::<T>().await?;

        Ok(data)
    }

    // 获取发布时间和专辑艺人
    async fn fetch_album_info(&self, album_id: &str) -> Result<(String, String), ProviderError> {
        let mut url = self.search_url.clone();
        {
            let mut params = url.query_pairs_mut();
            params
                .append_pair("pn", "1")
                .append_pair("rn", "1")
                .append_pair("albumid", album_id)
                .append_pair("stype", "albuminfo")
                .append_pair("showall", "0")
                .append_pair("pcjson", "1")
                .append_pair("encoding", "utf8");
        }

        let info: AlbumInfo = self.get_json(url).await?;

        Ok((info.date.trim().to_string(), info.artist.trim().to_string()))
    }

    async fn album_info(&self, album_id: &str) -> (String, String) {
        let now = Instant::now();
        if let Some(cached) = self.album_cache.read().await.get(album_id)
            && cached.expires_at > now
        {
            return cached.value.clone();
        }

        let (value, ttl) =
            match tokio::time::timeout(ALBUM_INFO_REQUEST_TIMEOUT, self.fetch_album_info(album_id))
                .await
            {
                Ok(Ok(value)) => (value, ALBUM_CACHE_TTL),
                Ok(Err(error)) => {
                    tracing::debug!(album_id, %error, "获取专辑信息失败，暂时使用搜索结果元数据");
                    ((String::new(), String::new()), ALBUM_CACHE_ERROR_TTL)
                }
                Err(_) => {
                    tracing::debug!(album_id, "获取专辑信息超时，暂时使用搜索结果元数据");
                    ((String::new(), String::new()), ALBUM_CACHE_ERROR_TTL)
                }
            };

        let mut cache = self.album_cache.write().await;
        cache.retain(|_, cached| cached.expires_at > now);
        if cache.len() >= ALBUM_CACHE_CAPACITY
            && let Some(key) = cache.keys().next().cloned()
        {
            cache.remove(&key);
        }
        cache.insert(
            album_id.to_owned(),
            CachedAlbumInfo {
                value: value.clone(),
                expires_at: now + ttl,
            },
        );
        value
    }
}

#[async_trait]
impl Provider for KuwoProvider {
    fn id(&self) -> &'static str {
        "kuwo"
    }
    async fn search(
        &self,
        query: &str,
        page: i64,
        limit: i64,
    ) -> Result<SearchResult, ProviderError> {
        if page <= 0 {
            return Err(ProviderError::InvalidArgument(
                "page必须大于等于1".to_string(),
            ));
        }
        if limit <= 0 {
            return Err(ProviderError::InvalidArgument(
                "limit必须大于等于1".to_string(),
            ));
        }
        let mut url = self.search_url.clone();

        {
            let mut params = url.query_pairs_mut();
            params
                .append_pair("client", "kt")
                .append_pair("encoding", "utf8")
                .append_pair("rformat", "json")
                .append_pair("mobi", "1")
                .append_pair("vipver", "1")
                .append_pair("pn", &(page - 1).to_string())
                .append_pair("rn", &limit.to_string())
                .append_pair("correct", "1")
                .append_pair("all", query)
                .append_pair("ft", "music");
        }

        let resp: SearchResponse = self.get_json(url).await?;
        // 同一页只请求一次相同专辑，并以有限并发补充发布时间和专辑艺人。
        let mut seen_album_ids = HashSet::new();
        let album_ids = resp
            .abs_list
            .iter()
            .map(|item| item.album_id.trim())
            // 酷我会用 "0" 表示歌曲没有可查询的专辑，不能把它当成真实专辑 ID。
            .filter(|album_id| is_valid_album_id(album_id))
            .filter(|album_id| seen_album_ids.insert((*album_id).to_owned()))
            .take(MAX_ALBUM_INFO_PER_SEARCH)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let album_infos = stream::iter(album_ids)
            .map(|album_id| async move {
                let info = self.album_info(&album_id).await;
                (album_id, info)
            })
            .buffer_unordered(ALBUM_INFO_CONCURRENCY)
            .collect::<HashMap<_, _>>()
            .await;

        let mut tracks = Vec::with_capacity(resp.abs_list.len());
        for item in resp.abs_list {
            let id = item.music_rid.trim_start_matches("MUSIC_").to_string();
            let qualities = parse_qualities(&item.music_info, &item.format);
            let info = album_infos
                .get(item.album_id.trim())
                .cloned()
                .unwrap_or_default();
            let album_artist = if info.1.is_empty() {
                item.album_artist.clone()
            } else {
                info.1.clone()
            };

            tracks.push(SearchTrack {
                id,
                provider: self.id().to_string(),
                name: item.name,
                artists: split_non_empty(&item.artist, "&"),
                album: item.album,
                album_artist,
                date: info.0,
                cover_url: cover_url(&item.album_picture),
                duration_ms: duration_millis(&item.duration),
                qualities,
            });
        }

        Ok(SearchResult {
            provider: self.id().to_string(),
            query: query.to_string(),
            page,
            limit,
            total: resp.total,
            tracks,
        })
    }

    async fn resolve(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<ResolvedResource, ProviderError> {
        // 只接受声明过的音质
        let q = find_quality(quality_id);
        let Some(quality) = q else {
            return Err(ProviderError::UnsupportedQuality(quality_id.to_string()));
        };
        if track_id.is_empty() {
            return Err(ProviderError::InvalidArgument("空的track id".to_string()));
        }

        let mut url = self.resolve_url.clone();
        {
            let mut params = url.query_pairs_mut();
            params
                .append_pair("f", "web")
                .append_pair("user", "0")
                .append_pair("source", "kwplayer_ar_5.0.0.0_B_jiakong_vh.apk")
                .append_pair("type", "convert_url_with_sign")
                .append_pair("rid", track_id)
                .append_pair("br", quality.id);
        }

        let resp: ResolveResponse = self.get_json(url).await?;
        if resp.data.url.is_empty() {
            return Err(ProviderError::InvalidResponse(
                "资源下载url为空".to_string(),
            ));
        }

        Ok(ResolvedResource {
            provider: self.id(),
            track_id: track_id.to_string(),
            quality,
            url: resp.data.url,
            headers: HashMap::from([("User-Agent".to_string(), USER_AGENT.to_string())]),
        })
    }
    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError> {
        let mut url = self.lyric_url.clone();

        {
            let mut params = url.query_pairs_mut();
            params.append_pair("musicId", track_id);
        }

        let resp: LyricResponse = self.get_json(url).await?;
        let mut lyrics = String::new();
        for line in resp.data.lines {
            let seconds: f64 = match line.time.parse() {
                Ok(v) if v >= 0.0 => v,
                _ => continue,
            };

            let hundredths = (seconds * 100.0).round() as i64;
            let _ = writeln!(
                &mut lyrics,
                "[{:02}:{:02}.{:02}]{}",
                hundredths / 6000,
                hundredths / 100 % 60,
                hundredths % 100,
                line.text
            );
        }

        Ok(lyrics.trim().to_string())
    }
}

static QUALITIES: &[QualitySpec] = &[
    QualitySpec {
        id: PREVIEW_QUALITY_ID,
        label: "MP3 128K",
        format: "mp3",
        bitrate: 128,
    },
    QualitySpec {
        id: DOWNLOAD_QUALITY_ID,
        label: "FLAC",
        format: "flac",
        bitrate: 2000,
    },
];

fn parse_config_url(value: &str, name: &str) -> Result<Url, ProviderError> {
    let url = Url::parse(value)
        .map_err(|error| ProviderError::InvalidArgument(format!("解析{name}地址失败: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ProviderError::InvalidArgument(format!(
            "{name}只允许HTTP或HTTPS地址"
        )));
    }
    Ok(url)
}

fn find_quality(id: &str) -> Option<QualitySpec> {
    QUALITIES.iter().copied().find(|q| q.id == id)
}

fn parse_qualities(info: &str, formats: &str) -> Vec<Quality> {
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(5);
    let mut add = |id: &str| {
        if !seen.contains(id)
            && let Some(q) = find_quality(id)
        {
            seen.insert(id.to_string());
            result.push(q.into())
        }
    };

    for part in info.split([';', ',']) {
        let part = part.trim();
        let pair: Vec<&str> = part.splitn(2, ':').collect();
        if pair.len() != 2 {
            continue;
        }
        if pair[0].trim().eq_ignore_ascii_case("bitrate") && pair[1].trim() == "2000" {
            add(DOWNLOAD_QUALITY_ID);
        }
    }

    let formats_lower = formats.to_lowercase();
    if formats_lower.contains("flac") {
        add(DOWNLOAD_QUALITY_ID);
    }

    result
}

fn split_non_empty(value: &str, separator: &str) -> Vec<String> {
    value
        .split(separator)
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

fn is_valid_album_id(album_id: &str) -> bool {
    !album_id.is_empty() && album_id != "0"
}

fn cover_url(path: &str) -> String {
    let mut p = path.trim();
    if p.is_empty() {
        return String::new();
    }

    if p.starts_with("http://") || p.starts_with("https://") {
        return p.to_string();
    }

    p = p.strip_prefix('/').unwrap_or(p);
    p = p.strip_prefix("120/").unwrap_or(p);

    format!("{}{}", COVER_URL, p)
}

fn duration_millis(value: &str) -> i64 {
    let seconds = value.trim().parse().unwrap_or(0.0);
    (seconds * 1000.0) as i64
}

fn flexible_int<'d, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'d>,
{
    let value = Value::deserialize(deserializer)?;
    let result = match value {
        Value::Number(num) => num.as_f64().unwrap_or(0.0) as usize,
        Value::String(s) => s.parse::<usize>().unwrap_or(0),
        _ => 0,
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{DOWNLOAD_QUALITY_ID, is_valid_album_id, parse_config_url, parse_qualities};

    #[test]
    fn validates_configured_http_urls() {
        assert!(parse_config_url("https://search.kuwo.cn/r.s", "测试API").is_ok());
        assert!(parse_config_url("ftp://search.kuwo.cn/r.s", "测试API").is_err());
        assert!(parse_config_url("not a url", "测试API").is_err());
    }

    #[test]
    fn rejects_missing_album_ids() {
        assert!(!is_valid_album_id(""));
        assert!(!is_valid_album_id("0"));
        assert!(is_valid_album_id("12345"));
    }

    #[test]
    fn exposes_only_flac_as_download_quality() {
        let qualities = parse_qualities(
            "bitrate:128;bitrate:192;bitrate:320;bitrate:1000;bitrate:2000",
            "mp3|ape|flac",
        );

        assert_eq!(
            qualities
                .iter()
                .map(|quality| quality.id.as_str())
                .collect::<Vec<_>>(),
            [DOWNLOAD_QUALITY_ID]
        );
    }
}
