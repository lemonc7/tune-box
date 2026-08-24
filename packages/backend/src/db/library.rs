use contracts::library::Song;
use sqlx::{QueryBuilder, Row, Sqlite, types::Json};

use crate::db::{Metadata, Repository, ScannedSong};

const MAX_SEARCH_KEYWORDS: usize = 5;

impl Repository {
    // 原子写入曲库数据并完成下载任务
    pub async fn complete_with_song(
        &self,
        id: i64,
        version: i64,
        final_path: &str,
        size: i64,
        metadata: &Metadata,
    ) -> Result<bool, sqlx::Error> {
        // 使用事务原子处理
        // tx实现了Drop trait，离开作用域会自动rollback(除非使用了commit)
        let mut tx = self.pool.begin().await?;

        // 标记下载完成
        let updated = sqlx::query!(
            r#"
            UPDATE downloads
            SET status = 'completed',
                path = ?,
                retry_count = 0,
                next_retry_at = NULL,
                error_message = NULL,
                finished_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
              AND status = 'tagging'
              AND version = ?
            "#,
            final_path,
            id,
            version,
        )
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 0 {
            return Ok(false);
        }

        // 插入音乐
        let song_id = sqlx::query_scalar!(
            r#"
            INSERT INTO songs (path, name, size, download_id)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(path) DO UPDATE SET
                name = excluded.name,
                size = excluded.size,
                download_id = excluded.download_id,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id
            "#,
            final_path,
            metadata.name,
            size,
            id,
        )
        .fetch_one(&mut *tx)
        .await?;

        // 先删除歌曲对应的歌手，再插入新的，防止旧数据残留
        sqlx::query!("DELETE FROM song_artists WHERE song_id = ?", song_id)
            .execute(&mut *tx)
            .await?;

        // 插入歌手
        for artist in &metadata.artists {
            let artist = artist.trim();
            if artist.is_empty() {
                continue;
            }

            sqlx::query!(
                r#"
                INSERT INTO song_artists (song_id, artist)
                VALUES (?, ?)
                ON CONFLICT(song_id, artist) DO NOTHING
                "#,
                song_id,
                artist,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.notify_download_changed(id);
        Ok(true)
    }

    // 获取歌曲记录，支持动态查询
    pub async fn list_songs(
        &self,
        keywords: &[&str],
        before_id: i64,
        limit: i64,
    ) -> Result<Vec<Song>, sqlx::Error> {
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT
                s.id,
                s.path,
                s.name,
                s.size,
                COALESCE(
                    (
                        SELECT json_group_array(ordered.artist)
                        FROM (
                            SELECT sa.artist
                            FROM song_artists AS sa
                            WHERE sa.song_id = s.id
                            ORDER BY sa.artist
                        ) AS ordered
                    ),
                    '[]'
                ) AS artists
            FROM songs AS s
            WHERE s.id <
            "#,
        );
        query.push_bind(before_id);

        let keywords = keywords
            .iter()
            .map(|k| k.trim())
            .filter(|k| !k.is_empty())
            .take(MAX_SEARCH_KEYWORDS)
            // SQLite 内置 lower() 只处理 ASCII；常量在 Rust 侧转换一次，避免每行重复调用。
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();

        for keyword in &keywords {
            query.push(
                r#"
                AND (
                    instr(lower(s.name),
                "#,
            );
            query.push_bind(keyword);

            query.push(
                r#"
                ) > 0
                OR EXISTS (
                    SELECT 1
                    FROM song_artists AS sa
                    WHERE sa.song_id = s.id
                      AND instr(lower(sa.artist),
                "#,
            );

            query.push_bind(keyword);

            query.push(
                r#"
                    ) > 0
                  )
                )
                "#,
            );
        }

        query.push(" ORDER BY s.id DESC LIMIT ");
        query.push_bind(limit.clamp(1, 101));

        query
            .build()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| {
                let path: String = row.try_get("path")?;
                let extension = std::path::Path::new(&path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                Ok(Song {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    artists: row.try_get::<Json<Vec<String>>, _>("artists")?.0,
                    extension,
                    size: row.try_get("size")?,
                })
            })
            .collect()
    }

    // 获取歌曲总数
    pub async fn count_songs(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!("SELECT COUNT(*) FROM songs")
            .fetch_one(&self.pool)
            .await
    }

    // 同步本地曲库
    pub async fn sync_library(
        &self,
        songs: &[ScannedSong],
        scan_token: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        for song in songs {
            // 先插入/更新歌曲
            let song_id = sqlx::query_scalar!(
                r#"
                INSERT INTO songs (
                    path, name, size, scan_token, download_id
                )
                VALUES (
                    ?, ?, ?, ?, 
                    (
                        SELECT id FROM downloads
                        WHERE path = ? AND status = 'completed'
                        LIMIT 1
                    )
                )
                ON CONFLICT(path) DO UPDATE SET
                    name = excluded.name,
                    size = excluded.size,
                    scan_token = excluded.scan_token,
                    download_id = COALESCE(songs.download_id, excluded.download_id),
                    updated_at = CURRENT_TIMESTAMP
                RETURNING id
                "#,
                song.path,
                song.name,
                song.size,
                scan_token,
                song.path,
            )
            .fetch_one(&mut *tx)
            .await?;

            // 删除歌手再添加
            sqlx::query!("DELETE FROM song_artists WHERE song_id = ?", song_id)
                .execute(&mut *tx)
                .await?;

            for artist in song
                .artists
                .iter()
                .map(|artist| artist.trim())
                .filter(|artist| !artist.is_empty())
            {
                sqlx::query!(
                    r#"
                INSERT INTO song_artists (song_id, artist)
                VALUES (?, ?)
                ON CONFLICT(song_id, artist) DO NOTHING
                "#,
                    song_id,
                    artist,
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        // 先删除不存在的歌曲对应的下载记录
        sqlx::query!(
            r#"
            DELETE FROM downloads
            WHERE id IN (
                SELECT download_id FROM songs
                WHERE scan_token != ?
                  AND download_id IS NOT NULL
            )
            "#,
            scan_token,
        )
        .execute(&mut *tx)
        .await?;

        // 删除不存在的歌曲记录
        sqlx::query!("DELETE FROM songs WHERE scan_token != ?", scan_token)
            .execute(&mut *tx)
            .await?;

        tx.commit().await
    }

    pub async fn get_song_path(&self, id: i64) -> Result<Option<String>, sqlx::Error> {
        Ok(sqlx::query!("SELECT path FROM songs WHERE id = ?", id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| row.path))
    }

    // 删除歌曲和对应下载记录
    pub async fn delete_song_with_download(&self, id: i64) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // 获取download id
        let row = sqlx::query!("SELECT download_id FROM songs WHERE id = ?", id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let download_id = row.download_id;

        // 删除歌曲
        sqlx::query!("DELETE FROM songs WHERE id = ?", id)
            .execute(&mut *tx)
            .await?;
        // 删除记录
        if let Some(download_id) = download_id {
            sqlx::query!("DELETE FROM downloads WHERE id = ?", download_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        if let Some(download_id) = download_id {
            self.notify_download_changed(download_id);
        }
        Ok(true)
    }
}
