CREATE TABLE downloads (
    id INTEGER PRIMARY KEY,

    -- 来源信息
    provider TEXT NOT NULL,
    track_id TEXT NOT NULL,

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
    -- Worker 每次领取时递增，作为防止旧 Worker 提交的版本号
    version INTEGER NOT NULL DEFAULT 0 CHECK(version >= 0),
    -- 当前处理阶段连续失败次数；进入下一阶段后清零
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK(retry_count >= 0),
    -- NULL 表示可以立即领取，否则需等待到指定时间
    next_retry_at DATETIME,

    path TEXT,
    error_message TEXT,

    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at DATETIME,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(provider, track_id)
);

CREATE TABLE songs (
    id INTEGER PRIMARY KEY,
    -- 程序下载的歌曲绑定对应任务；扫描导入的歌曲可以为空
    download_id INTEGER REFERENCES downloads(id) ON DELETE SET NULL,

    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size >= 0),
    -- 扫描曲库时用于识别已经不存在的文件
    scan_token TEXT NOT NULL DEFAULT '',

    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE song_artists (
    song_id INTEGER NOT NULL,
    artist TEXT NOT NULL,
    PRIMARY KEY(song_id, artist),

    FOREIGN KEY(song_id) REFERENCES songs(id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX idx_downloads_status_retry_created
    ON downloads(status, next_retry_at, created_at, id);
-- 曲库同步按最终路径关联已完成的下载任务，只索引可参与关联的记录
CREATE INDEX idx_downloads_completed_path
    ON downloads(path)
    WHERE status = 'completed' AND path IS NOT NULL;
-- 一个下载任务最多对应一首曲库歌曲，NULL 可重复
CREATE UNIQUE INDEX idx_songs_download_id
    ON songs(download_id)
    WHERE download_id IS NOT NULL;
