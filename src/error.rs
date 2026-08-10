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
    #[error("请求错误: {0}")]
    Request(#[from] reqwest::Error),
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("资源链接无效")]
    InvalidUrl,
    #[error("资源文件过大")]
    TooLarge,
    #[error("下载资源为空")]
    Empty,
    #[error("http状态错误: {0}")]
    Http(reqwest::StatusCode),
}

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("无效封面图")]
    InvalidCoverUrl,
    #[error("http请求失败: {0}")]
    Http(#[from] reqwest::Error),
    #[error("请求封面图响应错误: {0}")]
    HttpStatus(reqwest::StatusCode),
    #[error("封面图为空或超过大小限制")]
    CoverSizeInvalid,
    #[error("不支持的封面图格式")]
    UnsupportedCoverFormat,
    #[error("lofty音乐标签错误: {0}")]
    Lofty(#[from] lofty::error::AllocationError),
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析音频文件失败: {0}")]
    ParseFile(#[from] lofty::error::FileParseError),
    #[error("编码文件失败: {0}")]
    Encoding(#[from] lofty::error::FileEncodingError),
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("无效的provider")]
    InvalidProvider,
    #[error("任务已存在")]
    AlreadyExists,
    #[error("metadata编码失败: {0}")]
    MetadataSerializationFailed(#[from] serde_json::Error),
    #[error("数据库操作出错: {0}")]
    Db(#[from] toasty::Error),
}
