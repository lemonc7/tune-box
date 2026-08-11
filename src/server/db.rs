use jiff::Timestamp;

use crate::model::Metadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
pub enum Status {
    Pending,
    Resolving,
    Downloading,
    Tagging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, toasty::Model)]
#[unique(provider, track_id, quality)]
pub struct Download {
    #[key]
    #[auto]
    pub id: i64,
    pub provider: String,
    pub track_id: String,
    pub quality: String,
    #[column(type = text)]
    pub metadata: toasty::Json<Metadata>,
    #[index]
    #[default(Status::Pending)]
    pub status: Status,
    #[default(0)]
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    #[default(0)]
    pub attempt: i64,
    pub resource_url: Option<String>,
    pub path: Option<String>,
    pub error_message: Option<String>,
    #[index]
    #[auto]
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    #[auto]
    pub updated_at: Timestamp,
}

#[derive(Debug, toasty::Model)]
pub struct Artist {
    #[key]
    #[auto]
    pub id: i64,
    #[unique]
    pub name: String,

    #[has_many]
    pub song_artists: toasty::Deferred<Vec<SongArtist>>,
    #[has_many(via = song_artists.song)]
    pub songs: toasty::Deferred<Vec<Song>>,
}

#[derive(Debug, toasty::Model)]
pub struct Song {
    #[key]
    #[auto]
    pub id: i64,
    #[unique]
    #[column("path")]
    pub file_path: String,
    #[index]
    pub name: String,
    pub extension: String,
    pub size: i64,

    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
    #[index]
    #[default(String::default())]
    pub scan_token: String,

    #[has_many]
    pub song_artists: toasty::Deferred<Vec<SongArtist>>,
    #[has_many(via = song_artists.artist)]
    pub artists: toasty::Deferred<Vec<Artist>>,
}

#[derive(Debug, toasty::Model)]
#[key(song_id, artist_id)]
pub struct SongArtist {
    pub song_id: i64,
    #[index]
    pub artist_id: i64,
    #[belongs_to(key = song_id, references = id)]
    pub song: toasty::Deferred<Song>,
    #[belongs_to(key = artist_id, references = id)]
    pub artist: toasty::Deferred<Artist>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> Metadata {
        Metadata {
            name: "Song".to_string(),
            artists: vec!["Artist".to_string()],
            album: "Album".to_string(),
            album_artist: "Artist".to_string(),
            date: "2026".to_string(),
            cover_url: String::new(),
        }
    }

    #[tokio::test]
    async fn creates_download_with_unknown_total_and_zero_progress() {
        let mut db = toasty::Db::builder()
            .models(toasty::models!(Download, Artist))
            .connect("sqlite://memory")
            .await
            .unwrap();
        db.push_schema().await.unwrap();

        let download = Download::create()
            .provider("kuwo")
            .track_id("123")
            .quality("flac")
            .metadata(metadata())
            .exec(&mut db)
            .await
            .unwrap();

        assert_eq!(download.status, Status::Pending);
        assert_eq!(download.downloaded_bytes, 0);
        assert_eq!(download.total_bytes, None);
        assert_eq!(download.attempt, 0);
        assert_eq!(download.metadata.name, "Song");
    }
}
