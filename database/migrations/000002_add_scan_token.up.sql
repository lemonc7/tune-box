ALTER TABLE songs ADD COLUMN scan_token TEXT NOT NULL DEFAULT '';
CREATE INDEX idx_songs_scan_token ON songs(scan_token);
