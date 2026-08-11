CREATE TABLE downloads (
    id INTEGER PRIMARY KEY,

    -- 来源信息
    provider TEXT NOT NULL,
    track_id TEXT NOT NULL,
    quality TEXT NOT NULL,

    -- 搜索阶段取得的元数据快照，保存为 JSON 文本
    metadata TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata)),

    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN (
            'pending',
            'resolving',
            'downloading',
            'downloaded',
            'tagging',
            'paused',
            'completed',
            'failed',
            'cancelled'
        )),

    -- 下载进度
    downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK(downloaded_bytes >= 0),
    total_bytes INTEGER CHECK(total_bytes IS NULL OR total_bytes >= 0),
    -- 执行次数，仅用于重试和退避
    attempt INTEGER NOT NULL DEFAULT 0 CHECK(attempt >= 0),

    resource_url TEXT,
    path TEXT,
    error_message TEXT,

    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at DATETIME,
    finished_at DATETIME,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(provider, track_id, quality)
);

CREATE TABLE songs (
    id INTEGER PRIMARY KEY,

    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    extension TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size >= 0),
    -- 扫描曲库时用于识别已经不存在的文件
    scan_token TEXT NOT NULL DEFAULT '',

    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE artists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE song_artists (
    song_id INTEGER NOT NULL,
    artist_id INTEGER NOT NULL,

    PRIMARY KEY(song_id, artist_id),
    FOREIGN KEY(song_id) REFERENCES songs(id) ON DELETE CASCADE,
    FOREIGN KEY(artist_id) REFERENCES artists(id) ON DELETE CASCADE
) WITHOUT ROWID;

-- worker 获取待执行任务时使用
CREATE INDEX idx_downloads_status_created ON downloads(status, created_at, id);
CREATE INDEX idx_songs_name ON songs(name);
CREATE INDEX idx_artists_name ON artists(name);
CREATE INDEX idx_songs_scan_token ON songs(scan_token);
-- PRIMARY KEY(song_id, artist_id) 已覆盖 song_id 查询，
-- 反向按 artist_id 查询需要额外索引。
CREATE INDEX idx_song_artists_artist_id ON song_artists(artist_id);