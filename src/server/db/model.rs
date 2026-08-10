use jiff::Timestamp;

#[derive(Debug, toasty::Model)]
#[unique(provider, track_id, quality)]
pub struct Download {
    #[key]
    #[auto]
    pub id: i64,
    pub provider: String,
    pub track_id: String,
    pub quality: String,
    pub metadata: String,
    #[index]
    #[default("pending".to_string())]
    pub status: String,
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
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
}

#[derive(Debug, toasty::Model)]
pub struct Song {
    #[key]
    #[auto]
    pub id: i64,
    #[unique]
    pub file_path: String,
    #[index]
    pub name: String,
    pub extension: String,
    pub size: i64,

    pub created_at: String,
    pub updated_at: String,
    #[index]
    pub scan_token: String,

    #[has_many]
    pub song_artists: toasty::Deferred<Vec<SongArtist>>,
}

#[derive(Debug, toasty::Model)]
#[key(song_id, artist_id)]
pub struct SongArtist {
    #[index]
    pub song_id: i64,
    #[index]
    pub artist_id: i64,
    #[belongs_to(key = song_id, references = id)]
    pub song: toasty::Deferred<Song>,
    #[belongs_to(key = artist_id, references = id)]
    pub artist: Artist,
}
