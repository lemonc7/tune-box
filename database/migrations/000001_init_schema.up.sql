CREATE TABLE downloads (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  -- 来源
  provider TEXT NOT NULL,
  track_id TEXT NOT NULL,
  quality TEXT NOT NULL,

  -- metadata快照
  metadata TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN (
        'pending', 
        'resolving', 
        'downloading', 
        'tagging', 
        'paused', 
        'completed', 
        'failed', 
        'cancelled'
    )),
  
  -- 下载进度
  downloaded_bytes INTEGER NOT NULL DEFAULT 0,
  total_bytes INTEGER NOT NULL DEFAULT 0,
  -- 重试
  attempt INTEGER NOT NULL DEFAULT 0,
  resource_url TEXT,
  path TEXT,

  error_message TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  started_at TEXT,
  finished_at TEXT,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(provider, track_id, quality)
);

CREATE TABLE songs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  extension TEXT NOT NULL,
  size INTEGER NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE artists (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE song_artists (
  song_id INTEGER NOT NULL,
  artist_id INTEGER NOT NULL,
  PRIMARY KEY (song_id, artist_id),
  FOREIGN KEY (song_id) REFERENCES songs(id) ON DELETE CASCADE,
  FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE CASCADE
);

CREATE INDEX idx_download_status ON downloads(status);
CREATE INDEX idx_download_created_at ON downloads(created_at DESC);
CREATE INDEX idx_songs_name ON songs(name);
CREATE INDEX idx_artists_name ON artists(name);
CREATE INDEX idx_song_artists_artist_id ON song_artists(artist_id);