DROP INDEX IF EXISTS idx_songs_scan_token;
ALTER TABLE songs DROP COLUMN scan_token;
