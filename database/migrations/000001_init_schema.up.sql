CREATE TABLE artists (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE musics (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider TEXT NOT NULL,
  track_id TEXT NOT NULL,
  name TEXT NOT NULL,
  album TEXT,
  duration_ms INTEGER,
  cover_url TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(provider, track_id)
);

CREATE TABLE music_artists (
  music_id INTEGER NOT NULL,
  artist_id INTEGER NOT NULL,
  PRIMARY KEY (music_id, artist_id),
  FOREIGN KEY (music_id) REFERENCES musics(id) ON DELETE CASCADE,
  FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE CASCADE
);

CREATE TABLE downloads (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  music_id INTEGER NOT NULL,
  provider TEXT NOT NULL,
  track_id TEXT NOT NULL,
  quality TEXT NOT NULL,
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
  FOREIGN KEY (music_id) REFERENCES musics(id) ON DELETE CASCADE,
  UNIQUE(provider, track_id, quality)
);

CREATE TABLE library_files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  music_id INTEGER,
  path TEXT NOT NULL UNIQUE,
  file_name TEXT NOT NULL,
  extension TEXT NOT NULL,
  size INTEGER NOT NULL,
  modified_at TEXT NOT NULL,
  scan_token TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (music_id) REFERENCES musics(id) ON DELETE SET NULL
);

CREATE INDEX idx_music_artist_id ON music_artists(artist_id);
CREATE INDEX idx_music_name ON musics(name);
CREATE INDEX idx_artist_name ON artists(name);
CREATE INDEX idx_download_status ON downloads(status);
CREATE INDEX idx_download_created_at ON downloads(created_at DESC);
CREATE INDEX idx_library_file_name ON library_files(file_name);
