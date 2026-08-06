-- name: CreateDownload :one
INSERT INTO downloads (
  provider, track_id, quality, name, artists, album, duration_ms, cover_url, file_name
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(provider, track_id, quality) DO NOTHING
RETURNING *;

-- name: GetDownloadByKey :one
SELECT * FROM downloads
WHERE provider = ? 
  AND track_id = ? 
  AND quality = ?;

-- name: GetDownload :one
SELECT * FROM downloads WHERE id = ?;

-- name: ListPendingDownloadIDs :many
SELECT id FROM downloads WHERE status = 'pending' ORDER BY id;

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
SET progress = ?, 
    downloaded_bytes = ?, 
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
    file_name = ?,
    path = ?, 
    size = ?, 
    content_type = ?,
    progress = 100, 
    downloaded_bytes = ?, 
    total_bytes = ?,
    error_message = NULL, 
    finished_at = CURRENT_TIMESTAMP,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ? 
  AND status = 'tagging';

-- name: RequeueMissingDownload :execrows
UPDATE downloads
SET status = 'pending', 
    progress = 0, 
    downloaded_bytes = 0, 
    total_bytes = 0,
    resource_url = NULL, 
    path = NULL, 
    size = NULL, 
    content_type = NULL,
    error_message = NULL, 
    started_at = NULL, 
    finished_at = NULL,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ? 
  AND status = 'completed';

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
  file_name,
  extension,
  size, 
  modified_at, 
  scan_token
)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(file_name) DO UPDATE SET
  scan_token = excluded.scan_token,
  updated_at = CURRENT_TIMESTAMP;

-- name: DeleteLibraryFilesNotSeen :exec
DELETE FROM library_files WHERE scan_token <> ?;

-- name: DeleteLibraryFile :exec
DELETE FROM library_files WHERE id = ?;

-- name: LibraryFileExists :one
SELECT EXISTS(
  SELECT 1
  FROM library_files
  WHERE file_name = ?
);

-- name: FindLibraryFilesByName :many
SELECT * FROM library_files
WHERE file_name LIKE '%' || @name || '%';