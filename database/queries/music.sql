-- name: CreateDownload :one
INSERT INTO downloads (
  provider,
  track_id, 
  quality, 
  name, 
  artists, 
  album, 
  cover_url
)
VALUES (?, ?, ?, ?, ?, ?, ?)
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
    size = ?,
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
LIMIT ?;

-- name: UpsertLibraryFile :exec
INSERT INTO library_files (
  download_id,
  name, 
  artists, 
  file_name, 
  extension, 
  size, 
  modified_at
)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(download_id) DO UPDATE SET
  name = excluded.name,
  artists = excluded.artists,
  file_name = excluded.file_name,
  extension = excluded.extension,
  size = excluded.size,
  modified_at = excluded.modified_at,
  updated_at = CURRENT_TIMESTAMP;

-- name: RequeueMissingDownload :execrows
UPDATE downloads
SET status = 'pending',
    downloaded_bytes = 0,
    total_bytes = 0,
    resource_url = NULL,
    path = NULL,
    size = NULL,
    error_message = NULL,
    started_at = NULL,
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?
  AND status = 'completed';

-- name: SearchLibraryFiles :many
SELECT * FROM library_files
WHERE (
  name LIKE '%' || @keyword || '%'
  OR artists LIKE '%' || @keyword || '%'
)
ORDER BY created_at DESC
LIMIT @limit;