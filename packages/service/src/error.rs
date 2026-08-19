use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("数据库错误: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("数据库迁移错误: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("创建父目录错误: {0}")]
    CreateParent(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("网络错误: {0}")]
    Network(#[from] reqwest::Error),
    #[error("解析json错误: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("无效响应: {0}")]
    InvalidResponse(String),
    #[error("不支持的音质: {0}")]
    UnsupportedQuality(String),
    #[error("无效参数: {0}")]
    InvalidArgument(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("下载已中断")]
    Cancelled,
    #[error("请求错误: {0}")]
    Request(#[from] reqwest::Error),
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("资源链接无效")]
    InvalidUrl,
    #[error("文件名无效")]
    InvalidFileName,
    #[error("目标文件已存在")]
    TargetExists,
    #[error("断点续传响应范围无效")]
    InvalidRange,
    #[error("资源文件过大")]
    TooLarge,
    #[error("下载资源为空")]
    Empty,
    #[error("资源服务器返回了非音频内容: {0}")]
    UnexpectedContentType(String),
    #[error("下载文件大小不匹配，期望 {expected} 字节，实际 {actual} 字节")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("http状态错误: {0}")]
    Http(StatusCode),
}
