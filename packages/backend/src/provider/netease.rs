use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use async_trait::async_trait;
use contracts::music::{MusicResult, Track as SearchTrack};
use futures_util::{StreamExt, stream};
use reqwest::{StatusCode, Url, header};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::Value;
use time::{OffsetDateTime, UtcOffset};

use crate::{
    error::ProviderError,
    provider::{DOWNLOAD_QUALITY_ID, PREVIEW_QUALITY_ID, Provider, ResolvedResource, USER_AGENT},
};

const ALBUM_INFO_CONCURRENCY: usize = 6;
const MAX_ALBUM_INFO_PER_SEARCH: usize = 30;
const ALBUM_INFO_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_EXTENDED_RESPONSE_BYTES: u64 = 64 * 1024;

pub struct NeteaseProvider {
    client: reqwest::Client,
    base_url: Url,
    extended_resolve_url: Url,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    code: i32,
    result: Option<SearchData>,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    #[serde(rename = "songCount")]
    total: usize,
    #[serde(default)]
    songs: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct Track {
    id: i64,
    name: String,
    #[serde(default, rename = "ar")]
    artists: Vec<Artist>,
    #[serde(default, rename = "al")]
    album: Album,
    #[serde(default, rename = "publishTime")]
    publish_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct Artist {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct Album {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "picUrl")]
    picture_url: String,
}

#[derive(Debug, Deserialize)]
struct LyricResponse {
    #[serde(default)]
    code: i32,
    lrc: Option<LyricData>,
}

#[derive(Debug, Deserialize)]
struct LyricData {
    #[serde(default)]
    lyric: String,
}

#[derive(Debug, Deserialize)]
struct AlbumDetailResponse {
    #[serde(default)]
    code: i32,
    album: Option<AlbumDetail>,
}

#[derive(Debug, Deserialize)]
struct AlbumDetail {
    #[serde(default, rename = "publishTime")]
    publish_time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SongDetailResponse {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    songs: Vec<SongDetail>,
}

#[derive(Debug, Deserialize)]
struct SongDetail {
    id: i64,
    #[serde(default)]
    no: u32,
    #[serde(default, deserialize_with = "flexible_optional_string")]
    cd: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct SongTagMetadata {
    track_number: String,
    disc_number: String,
}

#[derive(Debug, Deserialize)]
struct DirectResolveResponse {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    data: Vec<DirectResource>,
}

#[derive(Debug, Deserialize)]
struct DirectResource {
    id: i64,
    url: Option<String>,
    #[serde(default)]
    br: u32,
    #[serde(default, rename = "type")]
    format: Option<String>,
    #[serde(default, rename = "freeTrialInfo")]
    free_trial_info: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ExtendedResolveResponse {
    url: Option<String>,
    #[serde(default, deserialize_with = "flexible_optional_u64")]
    br: Option<u64>,
    #[serde(default, deserialize_with = "flexible_optional_u64")]
    size: Option<u64>,
    #[serde(default, rename = "type")]
    format: Option<String>,
    #[serde(default, alias = "msg", alias = "error")]
    message: Option<String>,
}

impl NeteaseProvider {
    pub fn new(
        client: reqwest::Client,
        base_url: &str,
        extended_resolve_url: &str,
    ) -> Result<Self, ProviderError> {
        let mut base_url = parse_config_url(base_url, "网易云API")?;
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }

        let extended_resolve_url = parse_config_url(extended_resolve_url, "网易云扩展解析API")?;

        Ok(Self {
            client,
            base_url,
            extended_resolve_url,
        })
    }

    fn api_url(&self, path: &str) -> Result<Url, ProviderError> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .map_err(|error| {
                ProviderError::InvalidArgument(format!("解析网易云API地址失败: {error}"))
            })
    }

    async fn get_api<T>(&self, path: &str, parameters: &[(&str, &str)]) -> Result<T, ProviderError>
    where
        T: DeserializeOwned,
    {
        let mut url = self.api_url(path)?;
        {
            let mut query = url.query_pairs_mut();
            for &(name, value) in parameters {
                query.append_pair(name, value);
            }
        }

        let response = self
            .client
            .get(url)
            .header(header::USER_AGENT, USER_AGENT)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json::<T>().await?)
    }

    async fn fetch_album_date(&self, album_id: i64) -> Result<String, ProviderError> {
        let album_id = album_id.to_string();
        let payload: AlbumDetailResponse = self.get_api("album", &[("id", &album_id)]).await?;
        ensure_api_success(payload.code, "专辑详情")?;
        let publish_time = payload.album.and_then(|album| album.publish_time);
        Ok(format_publish_time(publish_time))
    }

    async fn fetch_song_tag_metadata(
        &self,
        song_ids: &[i64],
    ) -> Result<HashMap<i64, SongTagMetadata>, ProviderError> {
        if song_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let ids = song_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let payload: SongDetailResponse = self.get_api("song/detail", &[("ids", &ids)]).await?;
        ensure_api_success(payload.code, "歌曲详情")?;

        Ok(song_tag_metadata_from_details(payload.songs))
    }

    async fn song_tag_metadata(&self, song_ids: &[i64]) -> HashMap<i64, SongTagMetadata> {
        match tokio::time::timeout(
            ALBUM_INFO_REQUEST_TIMEOUT,
            self.fetch_song_tag_metadata(song_ids),
        )
        .await
        {
            Ok(Ok(metadata)) => metadata,
            Ok(Err(error)) => {
                tracing::debug!(%error, "获取网易云歌曲详情失败，曲目和光盘序号将留空");
                HashMap::new()
            }
            Err(_) => {
                tracing::debug!("获取网易云歌曲详情超时，曲目和光盘序号将留空");
                HashMap::new()
            }
        }
    }

    async fn album_date(&self, album_id: i64) -> String {
        match tokio::time::timeout(ALBUM_INFO_REQUEST_TIMEOUT, self.fetch_album_date(album_id))
            .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                tracing::debug!(album_id, %error, "获取网易云专辑日期失败，保留搜索结果元数据");
                String::new()
            }
            Err(_) => {
                tracing::debug!(album_id, "获取网易云专辑日期超时，保留搜索结果元数据");
                String::new()
            }
        }
    }

    async fn resolve_from_api(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<Option<String>, ProviderError> {
        let response: DirectResolveResponse = if quality_id == PREVIEW_QUALITY_ID {
            self.get_api("song/url", &[("id", track_id), ("br", "128000")])
                .await?
        } else {
            self.get_api("song/url/v1", &[("id", track_id), ("level", "lossless")])
                .await?
        };

        ensure_api_success(response.code, "音频解析")?;
        let expected_id = track_id.parse::<i64>().map_err(|_| {
            ProviderError::InvalidArgument("网易云歌曲ID必须是十进制整数".to_string())
        })?;
        direct_resource_url(response, expected_id, quality_id)
    }

    async fn resolve_extended(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<String, ProviderError> {
        let url = build_extended_resolve_url(&self.extended_resolve_url, track_id, quality_id);

        // 扩展解析服务是第三方地址；这里只发送通用 User-Agent，不发送账号 Cookie。
        let response = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::InvalidResponse(extended_http_status_error(
                status,
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_EXTENDED_RESPONSE_BYTES)
        {
            return Err(ProviderError::InvalidResponse(
                "网易云扩展解析接口响应过大".to_string(),
            ));
        }
        let body = response.bytes().await?;
        if body.len() as u64 > MAX_EXTENDED_RESPONSE_BYTES {
            return Err(ProviderError::InvalidResponse(
                "网易云扩展解析接口响应过大".to_string(),
            ));
        }
        let response =
            serde_json::from_slice::<ExtendedResolveResponse>(&body).map_err(|error| {
                ProviderError::InvalidResponse(format!(
                    "网易云扩展解析接口没有返回有效JSON: {error}"
                ))
            })?;

        if response.size == Some(0) {
            return Err(ProviderError::InvalidResponse(
                "网易云扩展解析接口返回的资源大小为0".to_string(),
            ));
        }
        let resource_url = response.url.as_deref().unwrap_or_default().trim();
        if resource_url.is_empty() {
            let detail = response
                .message
                .as_deref()
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .unwrap_or("没有错误详情");
            return Err(ProviderError::InvalidResponse(format!(
                "网易云扩展解析接口返回了空地址: {detail}"
            )));
        }

        validate_resource_url(
            resource_url,
            response.br,
            response.format.as_deref(),
            quality_id,
        )?;
        Ok(resource_url.to_string())
    }

    async fn resolve_with_fallback(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<String, ProviderError> {
        match self.resolve_from_api(track_id, quality_id).await {
            Ok(Some(resource_url)) => return Ok(resource_url),
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(
                    provider = self.id(),
                    track_id,
                    %error,
                    "网易云自建API没有返回所需音质，尝试扩展解析"
                );
            }
        }

        // Provider 不再自行重试。下载任务管理器已经统一执行指数退避重试；
        // 试听请求也不会因为两层重试叠加而长时间占用。
        self.resolve_extended(track_id, quality_id).await
    }
}

#[async_trait]
impl Provider for NeteaseProvider {
    fn id(&self) -> &'static str {
        "netease"
    }

    async fn search(
        &self,
        query: &str,
        page: i64,
        limit: i64,
    ) -> Result<MusicResult, ProviderError> {
        if query.trim().is_empty() {
            return Err(ProviderError::InvalidArgument(
                "搜索关键词不能为空".to_string(),
            ));
        }
        if page <= 0 || limit <= 0 {
            return Err(ProviderError::InvalidArgument(
                "page和limit必须大于等于1".to_string(),
            ));
        }
        let offset = (page - 1)
            .checked_mul(limit)
            .ok_or_else(|| ProviderError::InvalidArgument("搜索偏移量溢出".to_string()))?;
        let limit_string = limit.to_string();
        let offset_string = offset.to_string();
        let payload: SearchResponse = self
            .get_api(
                "cloudsearch",
                &[
                    ("keywords", query),
                    ("limit", &limit_string),
                    ("type", "1"),
                    ("offset", &offset_string),
                ],
            )
            .await?;
        ensure_api_success(payload.code, "搜索")?;
        let result = payload.result.ok_or_else(|| {
            ProviderError::InvalidResponse("网易云搜索响应缺少result".to_string())
        })?;
        let SearchData { total, songs } = result;

        let song_ids = songs.iter().map(|track| track.id).collect::<Vec<_>>();
        let song_tag_metadata = self.song_tag_metadata(&song_ids).await;

        // cloudsearch 对部分歌曲返回 publishTime=0；只为缺日期的专辑补查一次详情。
        let mut seen_album_ids = HashSet::new();
        let album_ids = songs
            .iter()
            .filter(|track| track.publish_time.unwrap_or_default() <= 0)
            .map(|track| track.album.id)
            .filter(|album_id| *album_id > 0)
            .filter(|album_id| seen_album_ids.insert(*album_id))
            .take(MAX_ALBUM_INFO_PER_SEARCH)
            .collect::<Vec<_>>();
        let album_dates = stream::iter(album_ids)
            .map(|album_id| async move { (album_id, self.album_date(album_id).await) })
            .buffer_unordered(ALBUM_INFO_CONCURRENCY)
            .collect::<HashMap<_, _>>()
            .await;
        let tracks = songs
            .into_iter()
            .map(|track| {
                let album_date = album_dates.get(&track.album.id).map(String::as_str);
                let tag_metadata = song_tag_metadata.get(&track.id);
                into_search_track(track, album_date, tag_metadata)
            })
            .collect();

        Ok(MusicResult {
            provider: self.id().to_string(),
            query: query.to_string(),
            page,
            limit,
            total,
            tracks,
        })
    }

    async fn resolve(
        &self,
        track_id: &str,
        quality_id: &str,
    ) -> Result<ResolvedResource, ProviderError> {
        let track_id = track_id.trim();
        if track_id.is_empty() || !track_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ProviderError::InvalidArgument(
                "网易云歌曲ID必须是十进制整数".to_string(),
            ));
        }

        let resource_url = self.resolve_with_fallback(track_id, quality_id).await?;
        Ok(ResolvedResource {
            provider: self.id(),
            track_id: track_id.to_string(),
            url: resource_url,
            headers: HashMap::from([
                ("User-Agent".to_string(), USER_AGENT.to_string()),
                ("Referer".to_string(), "https://music.163.com/".to_string()),
            ]),
        })
    }

    async fn lyrics(&self, track_id: &str) -> Result<String, ProviderError> {
        let track_id = track_id.trim();
        if track_id.is_empty() || !track_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ProviderError::InvalidArgument(
                "网易云歌曲ID必须是十进制整数".to_string(),
            ));
        }

        let payload: LyricResponse = self.get_api("lyric", &[("id", track_id)]).await?;
        ensure_api_success(payload.code, "歌词")?;
        Ok(payload
            .lrc
            .map(|lyrics| lyrics.lyric.trim().to_string())
            .unwrap_or_default())
    }
}

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

fn ensure_api_success(code: i32, operation: &str) -> Result<(), ProviderError> {
    if matches!(code, 0 | 200) {
        Ok(())
    } else {
        Err(ProviderError::InvalidResponse(format!(
            "网易云{operation}接口返回业务状态码{code}"
        )))
    }
}

fn extended_http_status_error(status: StatusCode) -> String {
    match status {
        StatusCode::TOO_MANY_REQUESTS => "网易云扩展解析触发GD接口频率限制（HTTP 429）".to_string(),
        StatusCode::SERVICE_UNAVAILABLE => "网易云扩展解析服务暂时不可用（HTTP 503）".to_string(),
        _ => format!("网易云扩展解析接口返回HTTP状态{status}"),
    }
}

fn flexible_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(Value::Null) => None,
        Some(Value::Number(value)) => value.as_u64(),
        Some(Value::String(value)) => value.trim().parse().ok(),
        Some(_) => None,
    })
}

fn flexible_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value),
        Some(Value::Number(value)) => Some(value.to_string()),
        Some(_) => None,
    })
}

fn song_tag_metadata_from_details(songs: Vec<SongDetail>) -> HashMap<i64, SongTagMetadata> {
    songs
        .into_iter()
        .map(|song| {
            let track_number = if song.no > 0 { song.no.to_string() } else { Default::default() };
            let disc_number = song
                .cd
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("null"))
                .unwrap_or_default()
                .to_string();
            (
                song.id,
                SongTagMetadata {
                    track_number,
                    disc_number,
                },
            )
        })
        .collect()
}

fn has_trial_info(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(value))
            if value.trim().is_empty() || value.trim().eq_ignore_ascii_case("null") =>
        {
            false
        }
        Some(_) => true,
    }
}

fn into_search_track(
    track: Track,
    album_date: Option<&str>,
    tag_metadata: Option<&SongTagMetadata>,
) -> SearchTrack {
    let date = {
        let search_date = format_publish_time(track.publish_time);
        if search_date.is_empty() {
            album_date.unwrap_or_default().to_string()
        } else {
            search_date
        }
    };
    let artists = track
        .artists
        .into_iter()
        .map(|artist| artist.name.trim().to_string())
        .filter(|artist| !artist.is_empty())
        .collect::<Vec<_>>();
    let album_artist = artists.first().cloned().unwrap_or_default();

    SearchTrack {
        id: track.id.to_string(),
        provider: "netease".to_string(),
        name: track.name,
        artists,
        album: track.album.name,
        album_artist,
        date,
        cover_url: normalize_cover_url(&track.album.picture_url),
        track_number: tag_metadata
            .map(|metadata| metadata.track_number.clone())
            .unwrap_or_default(),
        disc_number: tag_metadata
            .map(|metadata| metadata.disc_number.clone())
            .unwrap_or_default(),
    }
}

fn format_publish_time(timestamp_ms: Option<i64>) -> String {
    let Some(timestamp_ms) = timestamp_ms.filter(|value| *value > 0) else {
        return String::new();
    };
    let china_standard_time =
        UtcOffset::from_hms(8, 0, 0).expect("中国标准时间的UTC偏移量必须有效");
    OffsetDateTime::from_unix_timestamp(timestamp_ms.div_euclid(1000))
        .map(|timestamp| timestamp.to_offset(china_standard_time).date().to_string())
        .unwrap_or_default()
}

fn normalize_cover_url(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix("http://")
        .map(|rest| format!("https://{rest}"))
        .unwrap_or_else(|| value.to_string())
}

fn build_extended_resolve_url(base: &Url, track_id: &str, quality_id: &str) -> Url {
    let mut url = base.clone();
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in base.query_pairs() {
        if !matches!(key.as_ref(), "types" | "source" | "id" | "br") {
            serializer.append_pair(&key, &value);
        }
    }
    serializer.append_pair("types", "url");
    serializer.append_pair("source", "netease");
    serializer.append_pair("id", track_id);
    serializer.append_pair(
        "br",
        if quality_id == DOWNLOAD_QUALITY_ID {
            "999"
        } else {
            "128"
        },
    );
    url.set_query(Some(&serializer.finish()));
    url
}

fn direct_resource_url(
    response: DirectResolveResponse,
    expected_id: i64,
    quality_id: &str,
) -> Result<Option<String>, ProviderError> {
    let had_resources = !response.data.is_empty();
    let resource = response
        .data
        .into_iter()
        .find(|resource| resource.id == expected_id);
    let Some(resource) = resource else {
        return if had_resources {
            Err(ProviderError::InvalidResponse(
                "网易云自建API返回的歌曲ID与请求不一致".to_string(),
            ))
        } else {
            Ok(None)
        };
    };
    if quality_id == PREVIEW_QUALITY_ID && has_trial_info(resource.free_trial_info.as_ref()) {
        return Ok(None);
    }

    let resource_url = resource.url.unwrap_or_default();
    let resource_url = resource_url.trim();
    if resource_url.is_empty() {
        return Ok(None);
    }

    validate_http_resource_url(resource_url)?;
    if quality_id == DOWNLOAD_QUALITY_ID
        && !is_lossless_resource(
            resource_url,
            Some(u64::from(resource.br)),
            resource.format.as_deref(),
        )
    {
        return Ok(None);
    }

    Ok(Some(resource_url.to_string()))
}

fn validate_resource_url(
    value: &str,
    bitrate: Option<u64>,
    format: Option<&str>,
    quality_id: &str,
) -> Result<(), ProviderError> {
    validate_http_resource_url(value)?;
    if quality_id == DOWNLOAD_QUALITY_ID && !is_lossless_resource(value, bitrate, format) {
        return Err(ProviderError::InvalidResponse(
            "网易云扩展解析没有返回FLAC资源".to_string(),
        ));
    }
    Ok(())
}

fn validate_http_resource_url(value: &str) -> Result<Url, ProviderError> {
    let url = Url::parse(value)
        .map_err(|_| ProviderError::InvalidResponse("网易云解析返回了无效资源地址".to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ProviderError::InvalidResponse(
            "网易云资源地址只允许HTTP或HTTPS".to_string(),
        ));
    }
    Ok(url)
}

fn is_lossless_resource(value: &str, bitrate: Option<u64>, format: Option<&str>) -> bool {
    let path = Url::parse(value)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_default();
    if [".mp3", ".m4a", ".aac", ".ogg", ".opus", ".wav"]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        return false;
    }

    if let Some(format) = format.map(str::trim).filter(|format| !format.is_empty()) {
        return format.eq_ignore_ascii_case("flac");
    }
    if path.ends_with(".flac") {
        return true;
    }

    bitrate.is_some_and(|bitrate| (bitrate > 320 && bitrate < 10_000) || bitrate > 320_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_and_disc_numbers_from_song_detail() {
        let response: SongDetailResponse = serde_json::from_str(
            r#"{
                "code":200,
                "songs":[
                    {"id":347230,"no":3,"cd":"1/2"},
                    {"id":347231,"no":0,"cd":2},
                    {"id":347232,"no":8,"cd":"null"}
                ]
            }"#,
        )
        .unwrap();

        let metadata = song_tag_metadata_from_details(response.songs);
        assert_eq!(metadata[&347230].track_number, "3");
        assert_eq!(metadata[&347230].disc_number, "1/2");
        assert_eq!(metadata[&347231].track_number, "");
        assert_eq!(metadata[&347231].disc_number, "2");
        assert_eq!(metadata[&347232].track_number, "8");
        assert_eq!(metadata[&347232].disc_number, "");
    }
}
