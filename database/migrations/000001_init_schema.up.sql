CREATE TABLE artists (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
)


CREATE TABLE musics (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  album TEXT,
  duration INTEGER,
  track_number INTEGER,
  size INTEGER,
  release_date TEXT,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
)

CREATE TABLE music_artists (
  music_id INTEGER,
  artist_id INTEGER,
  PRIMARY KEY (music_id, artist_id),
  FOREIGN KEY (music_id) REFERENCES musics(id) ON DELETE CASCADE,
  FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE CASCADE
)

CREATE TABLE downloads (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  music_id INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN ('pending', 'downloading', 'completed', 'failed', 'paused')),
  provider TEXT NOT NULL,
  trackId TEXT NOT NULL,
  url TEXT NOT NULL UNIQUE,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  finished_at DATETIME,
  FOREIGN KEY (music_id) REFERENCES musics(id) ON DELETE CASCADE
)

CREATE INDEX idx_music_artist_id ON music_artists(artist_id);
CREATE INDEX idx_music_name ON musics(name);
CREATE INDEX idx_artist_name ON artists(name);

