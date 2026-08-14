pub mod codec;
pub mod download;
pub mod download_state;
pub mod library;

use std::{path::Path, str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use tokio::fs;

use crate::error::DatabaseError;

#[derive(Debug, Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn connect(db_url: &str) -> Result<SqlitePool, DatabaseError> {
    if let Some(path) = db_url.strip_prefix("sqlite://")
        && let Some(parent) = Path::new(path).parent()
    {
        fs::create_dir_all(parent).await?;
    }

    let options = SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
pub async fn connect_memory() -> Result<SqlitePool, DatabaseError> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::{MIGRATOR, connect_memory};

    #[tokio::test]
    async fn init_migration_creates_the_optimized_library_schema() {
        let pool = connect_memory().await.unwrap();

        let song_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO songs (path, name, extension, size) VALUES ('downloads/a.flac', 'a', 'flac', 1) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        for artist in ["second", "first"] {
            sqlx::query("INSERT INTO song_artists (song_id, artist) VALUES (?, ?)")
                .bind(song_id)
                .bind(artist)
                .execute(&pool)
                .await
                .unwrap();
        }

        // 已执行的初始化迁移再次运行应当安全跳过。
        MIGRATOR.run(&pool).await.unwrap();

        let table_sql = sqlx::query_scalar::<_, String>(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'song_artists'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(table_sql.contains("WITHOUT ROWID"));

        let artists = sqlx::query_scalar::<_, String>(
            "SELECT artist FROM song_artists WHERE song_id = ? ORDER BY artist",
        )
        .bind(song_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(artists, ["first", "second"]);

        let name_index = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_songs_name'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(name_index, 0);

        let redundant_artist_index = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN ('idx_song_artists_song_id_id', 'idx_song_artists_artist_song_id')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(redundant_artist_index, 0);
    }
}
