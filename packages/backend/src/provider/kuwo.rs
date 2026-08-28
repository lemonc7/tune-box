use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    time::Duration,
};

use async_trait::async_trait;
use contracts::music::{MusicResult, Track as SearchTrack};
use futures_util::{StreamExt, stream};
use moka::future::Cache;
use reqwest::header;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    error::ProviderError,
    provider::{Provider, ResolvedResource, USER_AGENT},
};

const COVER_URL: &str = "https://img3.kuwo.cn/star/albumcover/500/";
const ALBUM_INFO_CONCURRENCY: usize = 6;
const MAX_ALBUM_INFO_PER_SEARCH: usize = 30;
const ALBUM_CACHE_CAPACITY: u64 = 1024;
const ALBUM_INFO_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const ALBUM_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

pub struct KuwoProvider {
    client: reqwest::Client,
    search_url: reqwest::Url,
    resolve_url: reqwest::Url,
    lyric_url: reqwest::Url,
    album_cache: Cache<String, AlbumMetadata>,
}

#[derive(Debug, Clone, Default)]
struct AlbumMetadata {
    date: String,
    artist: String,
    track_numbers: HashMap<String, String>,
    track_dates: HashMap<String, String>,
}

impl KuwoProvider {
    pub fn new(
        client: reqwest::Client,
        search_url: &str,
        resolve_url: &str,
        lyric_url: &str,
    ) -> Result<Self, ProviderError> {
        let album_cache = Cache::builder()
            .max_capacity(ALBUM_CACHE_CAPACITY)
            .time_to_live(ALBUM_CACHE_TTL)
            .build();

        Ok(Self {
            client,
            search_url: parse_url(search_url, "酷我搜索API")?,
            resolve_url: parse_url(resolve_url, "酷我解析API")?,
            lyric_url: parse_url(lyric_url, "酷我歌词API")?,
            album_cache,
        })
    }

    async fn get_json<T: DeserializeOwned>(&self, url: reqwest::Url) -> Result<T, ProviderError> {
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::REFERER, "https://www.kuwo.cn/")
            .send()
            .await?
            .error_for_status()?;
        let data = resp.json::<T>().await?;

        Ok(data)
    }

    // 获取发布时间、专辑艺人和曲目序号
    async fn fetch_album_info(&self, album_id: &str) -> Result<AlbumMetadata, ProviderError> {
        let mut url = self.search_url.clone();
        {
            let mut params = url.query_pairs_mut();
            params
                .append_pair("pn", "0")
                .append_pair("rn", "100")
                .append_pair("albumid", album_id)
                .append_pair("stype", "albuminfo")
                .append_pair("showall", "1")
                .append_pair("pcjson", "1")
                .append_pair("encoding", "utf8");
        }
        let info: AlbumInfo = self.get_json(url).await?;
        Ok(album_metadata(info))
    }

    async fn album_info(&self, album_id: &str) -> AlbumMetadata {
        // 如果有缓存且未到期，直接返回，如果发生错误，使用fallback
        if let Some(cached) = self.album_cache.get(album_id).await {
            return cached;
        }

        match tokio::time::timeout(ALBUM_INFO_REQUEST_TIMEOUT, self.fetch_album_info(album_id))
            .await
        {
            Ok(Ok(value)) => {
                // 成功，写入缓存
                self.album_cache
                    .insert(album_id.to_owned(), value.clone())
                    .await;
                value
            }
            Ok(Err(error)) => {
                tracing::debug!(album_id,%error,"获取专辑信息失败，暂时使用搜索结果元数据");
                AlbumMetadata::default()
            }
            Err(_) => {
                tracing::debug!(album_id, "获取专辑信息超时，暂时使用搜索结果元数据");
                AlbumMetadata::default()
            }
        }
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
    ) -> Result<MusicResult, ProviderError> {
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

        let mut seen_album_ids = HashSet::new();
        let album_ids = resp
            .abs_list
            .iter()
            .map(|item| item.album_id.trim())
            .filter(|&album_id| is_valid_album_id(album_id))
            .filter(|&album_id| seen_album_ids.insert(album_id))
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
            let mut album_date = String::new();
            let mut album_artist = item.album_artist;

            let mut track_number = String::new();
            if let Some(info) = album_infos.get(item.album_id.trim()) {
                album_date = info
                    .track_dates
                    .get(&id)
                    .filter(|date| !date.is_empty())
                    .cloned()
                    .unwrap_or_else(|| info.date.clone());
                if !info.artist.is_empty() {
                    album_artist = info.artist.clone();
                }
                track_number = info.track_numbers.get(&id).cloned().unwrap_or_default();
            }

            tracks.push(SearchTrack {
                id,
                provider: self.id().to_string(),
                name: item.name,
                artists: split_non_empty(&item.artist, "&"),
                album: item.album,
                album_artist,
                date: album_date,
                cover_url: cover_url(&item.album_picture),
                track_number,
                disc_number: String::new(),
            });
        }

        Ok(MusicResult {
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
        let mut url = self.resolve_url.clone();
        {
            let mut params = url.query_pairs_mut();
            params
                .append_pair("f", "web")
                .append_pair("user", "0")
                .append_pair("source", "kwplayer_ar_5.0.0.0_B_jiakong_vh.apk")
                .append_pair("type", "convert_url_with_sign")
                .append_pair("rid", track_id)
                .append_pair("br", quality_id);
        }

        let resp: ResolvedResponse = self.get_json(url).await?;
        if resp.data.url.is_empty() {
            return Err(ProviderError::InvalidResponse(
                "资源下载url为空".to_string(),
            ));
        }
        Ok(ResolvedResource {
            provider: self.id(),
            track_id: track_id.to_string(),
            url: resp.data.url,
            headers: HashMap::from([(header::USER_AGENT.to_string(), USER_AGENT.to_string())]),
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
                line.text,
            );
        }
        Ok(lyrics.trim().to_string())
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(rename = "TOTAL", deserialize_with = "flexible_int")]
    total: usize,
    #[serde(rename = "abslist")]
    abs_list: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct ResolvedResponse {
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
    #[serde(rename = "MUSICRID")]
    music_rid: String,
    #[serde(rename = "NAME")]
    name: String,
    #[serde(rename = "web_albumpic_short")]
    album_picture: String,
}

#[derive(Debug, Deserialize)]
struct AlbumInfo {
    #[serde(default, rename = "pub")]
    date: String,
    #[serde(default, rename = "artist")]
    artist: String,
    #[serde(default, rename = "musiclist")]
    music_list: Vec<AlbumTrack>,
}

#[derive(Debug, Deserialize)]
struct AlbumTrack {
    #[serde(
        default,
        alias = "musicrid",
        alias = "MUSICRID",
        deserialize_with = "flexible_string"
    )]
    id: String,
    #[serde(default, deserialize_with = "flexible_string")]
    track: String,
    #[serde(default, rename = "releasedate", deserialize_with = "flexible_string")]
    release_date: String,
}

fn album_metadata(info: AlbumInfo) -> AlbumMetadata {
    let mut track_numbers = HashMap::new();
    let mut track_dates = HashMap::new();
    for track in info.music_list {
        let id = track.id.trim().trim_start_matches("MUSIC_");
        if id.is_empty() {
            continue;
        }
        let number = track.track.trim();
        if !number.is_empty() {
            track_numbers.insert(id.to_string(), number.to_string());
        }
        let release_date = track.release_date.trim();
        if !release_date.is_empty() {
            track_dates.insert(id.to_string(), release_date.to_string());
        }
    }
    AlbumMetadata {
        date: info.date.trim().to_string(),
        artist: info.artist.trim().to_string(),
        track_numbers,
        track_dates,
    }
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

fn flexible_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    })
}

fn parse_url(value: &str, name: &str) -> Result<reqwest::Url, ProviderError> {
    let url = reqwest::Url::parse(value)
        .map_err(|error| ProviderError::InvalidArgument(format!("解析{name}地址失败: {error}")))?;

    if !matches!(url.scheme(), "http" | "https") {
        return Err(ProviderError::InvalidArgument(format!(
            "{name}只允许HTTP或HTTPS地址"
        )));
    }
    Ok(url)
}

fn is_valid_album_id(album_id: &str) -> bool {
    !album_id.is_empty() && album_id != "0"
}

fn split_non_empty(value: &str, separator: &str) -> Vec<String> {
    value
        .split(separator)
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_numbers_from_album_info() {
        let info: AlbumInfo = serde_json::from_str(
            r#"{
                "pub":"1993-05-01",
                "artist":"Beyond",
                "musiclist":[
                    {"musicrid":"MUSIC_5886682","track":"6","releasedate":"1993-05-14"},
                    {"id":123,"track":7,"releasedate":"1993-05-15"}
                ],
                "tag":[
                    {"type":"Genre","cat1":"流行Pop","cat2":"流行摇滚Pop Rock"},
                    {"type":"Genre","cat1":"摇滚Rock","cat2":""},
                    {"type":"Language","cat1":"粤语","cat2":""}
                ]
            }"#,
        )
        .unwrap();

        let metadata = album_metadata(info);
        assert_eq!(metadata.date, "1993-05-01");
        assert_eq!(metadata.artist, "Beyond");
        assert_eq!(
            metadata.track_numbers.get("5886682"),
            Some(&"6".to_string())
        );
        assert_eq!(metadata.track_numbers.get("123"), Some(&"7".to_string()));
        assert_eq!(
            metadata.track_dates.get("5886682"),
            Some(&"1993-05-14".to_string())
        );
        assert_eq!(
            metadata.track_dates.get("123"),
            Some(&"1993-05-15".to_string())
        );
    }
}
