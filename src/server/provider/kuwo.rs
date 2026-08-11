use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::{
    error::ProviderError,
    model::{self, Quality, Resource, SearchResult},
    server::provider::Provider,
};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/124 Safari/537.36";
const COVER_URL: &str = "https://img3.kuwo.cn/star/albumcover/500/";

pub struct KuwoProvider {
    client: reqwest::Client,
    search_url: String,
    resolve_url: String,
    lyric_url: String,
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
    #[serde(rename = "allartistid")]
    artist_ids: String,
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
    ) -> Self {
        Self {
            client,
            search_url,
            resolve_url,
            lyric_url,
        }
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, ProviderError> {
        let resp = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://www.kuwo.cn/")
            .send()
            .await?;
        let data = resp.json::<T>().await?;

        Ok(data)
    }

    // 获取发布时间和专辑艺人
    async fn fetch_album_info(&self, album_id: &str) -> Result<(String, String), ProviderError> {
        let mut url = Url::parse(&self.search_url)
            .map_err(|e| ProviderError::InvalidArgument(format!("解析搜索url: {e}")))?;
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

        let info: AlbumInfo = self.get_json(url.as_str()).await?;

        Ok((info.date.trim().to_string(), info.artist.trim().to_string()))
    }
}

#[async_trait]
impl Provider for KuwoProvider {
    fn id(&self) -> &'static str {
        "kuwo"
    }
    fn name(&self) -> &'static str {
        "酷我"
    }
    async fn search(
        &self,
        query: &str,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, ProviderError> {
        if page == 0 {
            return Err(ProviderError::InvalidArgument(
                "page必须大于等于1".to_string(),
            ));
        }
        if limit == 0 {
            return Err(ProviderError::InvalidArgument(
                "limit必须大于等于1".to_string(),
            ));
        }
        let mut url = Url::parse(&self.search_url)
            .map_err(|e| ProviderError::InvalidArgument(format!("解析搜索url: {e}")))?;

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

        let resp: SearchResponse = self.get_json(url.as_str()).await?;
        let mut album_infos = HashMap::new();
        // 提前收集时间和艺人名称
        for item in &resp.abs_list {
            if item.album_id.is_empty() || album_infos.contains_key(&item.album_id) {
                continue;
            }
            match self.fetch_album_info(&item.album_id).await {
                Ok(info) => {
                    album_infos.insert(item.album_id.clone(), info);
                }
                Err(_) => {
                    album_infos.insert(item.album_id.clone(), ("".to_string(), "".to_string()));
                }
            }
        }

        let mut tracks = Vec::with_capacity(resp.abs_list.len());
        for item in resp.abs_list {
            let id = item.music_rid.trim_start_matches("MUSIC_").to_string();
            let qualities = parse_qualities(&item.music_info, &item.format);
            let info = album_infos.get(&item.album_id).cloned().unwrap_or_default();
            let album_artist = if info.1.is_empty() {
                item.album_artist.clone()
            } else {
                info.1.clone()
            };

            tracks.push(model::Track {
                id,
                provider: self.id(),
                name: item.name,
                artists: split_non_empty(&item.artist, "&"),
                artist_ids: split_non_empty(&item.artist_ids, "&"),
                album: item.album,
                album_id: item.album_id,
                album_artist,
                date: info.0,
                cover_url: cover_url(&item.album_picture),
                duration_ms: duration_millis(&item.duration),
                qualities,
            });
        }

        Ok(SearchResult {
            provider: self.id(),
            query: query.to_string(),
            page,
            limit,
            total: resp.total,
            tracks,
        })
    }

    async fn resolve(&self, track_id: &str, quality_id: &str) -> Result<Resource, ProviderError> {
        // 只接受声明过的音质
        let q = find_quality(quality_id);
        let Some(quality) = q else {
            return Err(ProviderError::UnsupportedQuality(quality_id.to_string()));
        };
        if track_id.is_empty() {
            return Err(ProviderError::InvalidArgument("空的track id".to_string()));
        }

        let mut url = Url::parse(&self.resolve_url)
            .map_err(|e| ProviderError::InvalidArgument(format!("解析解析url: {e}")))?;
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

        let resp: ResolveResponse = self.get_json(url.as_str()).await?;
        if resp.data.url.is_empty() {
            return Err(ProviderError::InvalidResponse(
                "资源下载url为空".to_string(),
            ));
        }

        Ok(Resource {
            provider: self.id(),
            track_id: track_id.to_string(),
            quality,
            url: resp.data.url,
            headers: HashMap::from([("User-Agent".to_string(), USER_AGENT.to_string())]),
        })
    }
    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError> {
        let mut url = Url::parse(&self.lyric_url)
            .map_err(|e| ProviderError::InvalidArgument(format!("解析歌词url: {e}")))?;

        {
            let mut params = url.query_pairs_mut();
            params.append_pair("musicId", track_id);
        }

        let resp: LyricResponse = self.get_json(url.as_str()).await?;
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

static QUALITIES: &[Quality] = &[
    Quality {
        id: "128kmp3",
        label: "MP3 128K",
        format: "mp3",
        bitrate: 128,
        lossless: false,
    },
    Quality {
        id: "192kmp3",
        label: "MP3 192K",
        format: "mp3",
        bitrate: 192,
        lossless: false,
    },
    Quality {
        id: "320kmp3",
        label: "MP3 320K",
        format: "mp3",
        bitrate: 320,
        lossless: false,
    },
    Quality {
        id: "1000kape",
        label: "APE",
        format: "ape",
        bitrate: 1000,
        lossless: true,
    },
    Quality {
        id: "2000kflac",
        label: "FLAC",
        format: "flac",
        bitrate: 2000,
        lossless: true,
    },
];

fn find_quality(id: &str) -> Option<Quality> {
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
            result.push(q)
        }
    };

    for part in info.split([';', ',']) {
        let part = part.trim();
        let pair: Vec<&str> = part.splitn(2, ':').collect();
        if pair.len() != 2 {
            continue;
        }
        if pair[0].trim().eq_ignore_ascii_case("bitrate") {
            match pair[1].trim() {
                "128" => add("128kmp3"),
                "192" => add("192kmp3"),
                "320" => add("320kmp3"),
                "1000" => add("1000kape"),
                "2000" => add("2000kflac"),
                _ => {}
            }
        }
    }

    let formats_lower = formats.to_lowercase();
    if formats_lower.contains("flac") {
        add("2000kflac");
    }
    if formats_lower.contains("ape") {
        add("1000kape");
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
