-- name: UpsertMusic :one
INSERT INTO musics (provider, track_id, name, album, duration_ms, cover_url)
VALUES (?, ?, ?, ?, ?, ?)
ON CONFLICT(provider, track_id) DO UPDATE SET
  name = CASE WHEN excluded.name <> '' THEN excluded.name ELSE musics.name END,
  album = COALESCE(excluded.album, musics.album),
  duration_ms = COALESCE(excluded.duration_ms, musics.duration_ms),
  cover_url = COALESCE(excluded.cover_url, musics.cover_url),
  updated_at = CURRENT_TIMESTAMP
RETURNING *;

-- name: CreateDownload :one
INSERT INTO downloads (music_id, provider, track_id, quality, file_name)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(provider, track_id, quality) DO NOTHING
RETURNING *;

-- name: GetDownloadByKey :one
SELECT * FROM downloads
WHERE provider = ? AND track_id = ? AND quality = ?;

-- name: GetDownload :one
SELECT * FROM downloads WHERE id = ?;

-- name: GetDownloadJob :one
SELECT d.id, d.music_id, d.provider, d.track_id, d.quality, d.status,
      d.progress, d.downloaded_bytes, d.total_bytes, d.attempt,
      d.resource_url, d.file_name, d.path, d.size, d.content_type,
      d.error_message, d.created_at, d.started_at, d.finished_at,
      m.name AS music_name, m.album AS music_album, m.cover_url AS music_cover_url,
      CAST(COALESCE(GROUP_CONCAT(a.name, ';'), '') AS TEXT) AS music_artists
FROM downloads d
JOIN musics m ON m.id = d.music_id
LEFT JOIN music_artists ma ON ma.music_id = m.id
LEFT JOIN artists a ON a.id = ma.artist_id
WHERE d.id = ?
GROUP BY d.id;

-- name: ListPendingDownloadIDs :many
SELECT id FROM downloads WHERE status = 'pending' ORDER BY id;

-- name: ResetInterruptedDownloads :exec
UPDATE downloads
SET status = 'pending', error_message = ?, finished_at = NULL
WHERE status IN ('resolving', 'downloading', 'tagging');

-- name: UpsertArtist :one
INSERT INTO artists (name)
VALUES (?)
ON CONFLICT(name) DO UPDATE SET name = excluded.name
RETURNING *;

-- name: LinkMusicArtist :exec
INSERT INTO music_artists (music_id, artist_id)
VALUES (?, ?)
ON CONFLICT(music_id, artist_id) DO NOTHING;

-- name: ClaimDownload :one
UPDATE downloads
SET status = 'resolving', started_at = CURRENT_TIMESTAMP, finished_at = NULL,
    error_message = NULL, attempt = attempt + 1
WHERE id = ? AND status = 'pending'
RETURNING *;

-- name: MarkDownloadDownloading :execrows
UPDATE downloads
SET status = 'downloading', resource_url = ?, finished_at = NULL
WHERE id = ? AND status = 'resolving';

-- name: UpdateDownloadProgress :exec
UPDATE downloads
SET progress = ?, downloaded_bytes = ?, total_bytes = ?
WHERE id = ? AND status = 'downloading';

-- name: MarkDownloadTagging :execrows
UPDATE downloads SET status = 'tagging'
WHERE id = ? AND status = 'downloading';

-- name: MarkDownloadCompleted :execrows
UPDATE downloads
SET status = 'completed', file_name = ?, path = ?, size = ?, content_type = ?,
    progress = 100, downloaded_bytes = ?, total_bytes = ?,
    error_message = NULL, finished_at = CURRENT_TIMESTAMP
WHERE id = ? AND status = 'tagging';

-- name: MarkDownloadFailed :exec
UPDATE downloads
SET status = 'failed', error_message = ?, finished_at = CURRENT_TIMESTAMP
WHERE id = ? AND status NOT IN ('paused', 'cancelled', 'completed');

-- name: PauseDownload :execrows
UPDATE downloads SET status = 'paused'
WHERE id = ? AND status IN ('pending', 'resolving', 'downloading');

-- name: CancelDownload :execrows
UPDATE downloads SET status = 'cancelled', error_message = ?, finished_at = CURRENT_TIMESTAMP
WHERE id = ? AND status IN ('pending', 'resolving', 'downloading', 'tagging', 'paused');

-- name: ResumeDownload :execrows
UPDATE downloads SET status = 'pending', error_message = NULL, finished_at = NULL
WHERE id = ? AND status = 'paused';

-- name: RetryDownload :execrows
UPDATE downloads
SET status = 'pending', resource_url = NULL, error_message = NULL,
    started_at = NULL, finished_at = NULL
WHERE id = ? AND status IN ('failed', 'cancelled');

-- name: DeleteDownload :execrows
DELETE FROM downloads WHERE id = ? AND status NOT IN ('resolving', 'downloading', 'tagging');

-- name: ListDownloads :many
SELECT d.id, d.music_id, d.provider, d.track_id, d.quality, d.status,
       d.progress, d.downloaded_bytes, d.total_bytes, d.attempt,
       d.resource_url, d.file_name, d.path, d.size, d.content_type,
       d.error_message, d.created_at, d.started_at, d.finished_at,
       m.name AS music_name, m.album AS music_album
FROM downloads d
JOIN musics m ON m.id = d.music_id
ORDER BY d.id DESC
LIMIT ?;

-- name: UpsertLibraryFile :exec
INSERT INTO library_files (
  music_id, path, file_name, extension, size, modified_at, scan_token
)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(path) DO UPDATE SET
  music_id = COALESCE(excluded.music_id, library_files.music_id),
  file_name = excluded.file_name,
  extension = excluded.extension,
  size = excluded.size,
  modified_at = excluded.modified_at,
  scan_token = excluded.scan_token,
  updated_at = CURRENT_TIMESTAMP;

-- name: DeleteLibraryFilesNotSeen :exec
DELETE FROM library_files WHERE scan_token <> ?;

-- name: ListLibraryFiles :many
SELECT lf.id, lf.music_id, lf.path, lf.file_name, lf.extension, lf.size,
      lf.modified_at, lf.scan_token, lf.created_at, lf.updated_at,
      m.provider, m.track_id, m.name AS music_name, m.album AS music_album
FROM library_files lf
LEFT JOIN musics m ON m.id = lf.music_id
ORDER BY lf.file_name COLLATE NOCASE
LIMIT ?;
