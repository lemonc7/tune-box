CREATE TABLE downloads (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider TEXT NOT NULL,
  track_id TEXT NOT NULL,
  quality TEXT NOT NULL,
  name TEXT NOT NULL,
  artists TEXT NOT NULL DEFAULT '[]',
  album TEXT,
  duration_ms INTEGER,
  cover_url TEXT,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN ('pending', 'resolving', 'downloading', 'tagging', 'paused', 'completed', 'failed', 'cancelled')),
  progress INTEGER NOT NULL DEFAULT 0 CHECK(progress BETWEEN 0 AND 100),
  downloaded_bytes INTEGER NOT NULL DEFAULT 0,
  total_bytes INTEGER NOT NULL DEFAULT 0,
  attempt INTEGER NOT NULL DEFAULT 0,
  resource_url TEXT,
  file_name TEXT,
  path TEXT,
  size INTEGER,
  content_type TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  started_at TEXT,
  finished_at TEXT,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(provider, track_id, quality)
);

CREATE TABLE library_files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  file_name TEXT NOT NULL UNIQUE,
  extension TEXT NOT NULL,
  size INTEGER NOT NULL,
  modified_at TEXT NOT NULL,
  scan_token TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_download_status ON downloads(status);
CREATE INDEX idx_download_created_at ON downloads(created_at DESC);