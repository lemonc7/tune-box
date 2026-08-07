-- name: CreateDownload :one
INSERT INTO downloads (
    provider, track_id, quality, metadata
)
VALUES (?, ?, ?, ?)
ON CONFLICT(provider, track_id, quality) DO NOTHING
RETURNING *;

-- name: GetDownloadByKey :one
SELECT * FROM downloads
WHERE provider = ?
  AND track_id = ?
  AND quality = ?;

-- name: GetDownload :one
SELECT * FROM downloads 
WHERE id = ?;

-- name: ListPendingDownloadIDs :many
SELECT id FROM downloads 
WHERE status = 'pending' 
ORDER BY id;

-- name: ResetInterruptedDownloads :exec
UPDATE downloads
SET status = 'pending',
    error_message = ?,
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE status IN ('resolving', 'downloading', 'tagging');

-- name: ClaimDownload :one
UPDATE downloads
SET status = 'resolving',
    started_at = CURRENT_TIMESTAMP,
    finished_at = NULL,
    error_message = NULL,
    attempt = attempt + 1,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'pending'
RETURNING *;

-- name: MarkDownloadDownloading :execrows
UPDATE downloads
SET status = 'downloading',
    resource_url = ?,
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'resolving';

-- name: UpdateDownloadProgress :exec
UPDATE downloads
SET downloaded_bytes = ?,
    total_bytes = ?,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'downloading';

-- name: MarkDownloadTagging :execrows
UPDATE downloads
SET status = 'tagging',
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'downloading';

-- name: MarkDownloadCompleted :execrows
UPDATE downloads
SET status = 'completed',
    path = ?,
    downloaded_bytes = ?,
    total_bytes = ?,
    error_message = NULL,
    finished_at = CURRENT_TIMESTAMP,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'tagging';

-- name: MarkDownloadFailed :exec
UPDATE downloads
SET status = 'failed',
    error_message = ?,
    finished_at = CURRENT_TIMESTAMP,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status NOT IN ('paused', 'cancelled', 'completed');

-- name: PauseDownload :execrows
UPDATE downloads
SET status = 'paused',
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status IN ('pending', 'resolving', 'downloading');

-- name: CancelDownload :execrows
UPDATE downloads
SET status = 'cancelled',
    error_message = ?,
    finished_at = CURRENT_TIMESTAMP,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status IN ('pending', 'resolving', 'downloading', 'tagging', 'paused');

-- name: ResumeDownload :execrows
UPDATE downloads
SET status = 'pending',
    error_message = NULL,
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'paused';

-- name: RetryDownload :execrows
UPDATE downloads
SET status = 'pending',
    resource_url = NULL,
    error_message = NULL,
    started_at = NULL,
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status IN ('failed', 'cancelled');

-- name: DeleteDownload :execrows
DELETE FROM downloads
WHERE id = ?
  AND status NOT IN ('resolving', 'downloading', 'tagging');

-- name: ListDownloads :many
SELECT * FROM downloads
ORDER BY id DESC
LIMIT @limit OFFSET @offset;

-- name: RequeueMissingDownload :execrows
UPDATE downloads
SET status = 'pending',
    downloaded_bytes = 0,
    total_bytes = 0,
    resource_url = NULL,
    path = NULL,
    error_message = NULL,
    started_at = NULL,
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'completed';

-- name: UpsertSong :one
INSERT INTO songs (path, name, extension, size)
VALUES (?, ?, ?, ?)
ON CONFLICT(path) DO UPDATE SET
    name = excluded.name,
    extension = excluded.extension,
    updated_at = CURRENT_TIMESTAMP,
    size = excluded.size
RETURNING id;

-- name: UpsertArtist :one
INSERT INTO artists (name) 
VALUES (?) ON CONFLICT(name) DO NOTHING 
RETURNING id;

-- name: AddSongArtist :exec
INSERT OR IGNORE INTO song_artists (song_id, artist_id) 
VALUES (?, ?);

-- name: ClearSongArtists :exec
DELETE FROM song_artists 
WHERE song_id = ?;

-- name: ListSongs :many
SELECT
    s.id,
    s.name,
    s.extension,
    GROUP_CONCAT(a.name, ', ') AS artists,
    s.size
FROM songs s
LEFT JOIN song_artists sa
    ON s.id = sa.song_id
LEFT JOIN artists a
    ON sa.artist_id = a.id
GROUP BY s.id
ORDER BY s.created_at DESC
LIMIT @limit OFFSET @offset;

-- name: SearchSongs :many
SELECT
    s.id,
    s.name,
    s.extension,
    GROUP_CONCAT(a.name, ', ') AS artists,
    s.size
FROM songs s
LEFT JOIN song_artists sa
    ON s.id = sa.song_id
LEFT JOIN artists a
    ON sa.artist_id = a.id
WHERE
    s.name LIKE '%' || @keyword1 || '%'
    OR a.name LIKE '%' || @keyword1 || '%'
GROUP BY s.id
ORDER BY s.created_at DESC
LIMIT @limit OFFSET @offset;

-- name: SearchSongs2 :many
SELECT
    s.id,
    s.name,
    s.extension,
    GROUP_CONCAT(a.name, ', ') AS artists,
    s.size
FROM songs s
LEFT JOIN song_artists sa
    ON s.id = sa.song_id
LEFT JOIN artists a
    ON sa.artist_id = a.id
WHERE
(
    s.name LIKE '%' || @keyword1 || '%'
    OR a.name LIKE '%' || @keyword1 || '%'
)
AND
(
    s.name LIKE '%' || @keyword2 || '%'
    OR a.name LIKE '%' || @keyword2 || '%'
)
GROUP BY s.id
ORDER BY s.created_at DESC
LIMIT @limit OFFSET @offset;
