-- 下载页按状态分类后使用 id 游标分页，表达式索引同时完成分类筛选和倒序读取
CREATE INDEX idx_downloads_category_id
    ON downloads(
        CASE status
            WHEN 'pending' THEN 0
            WHEN 'resolving' THEN 0
            WHEN 'downloading' THEN 0
            WHEN 'downloaded' THEN 0
            WHEN 'tagging' THEN 0
            WHEN 'failed' THEN 1
            WHEN 'paused' THEN 1
            ELSE 2
        END,
        id DESC
    );
