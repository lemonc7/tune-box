use std::io;

use axum::http;
use thiserror::Error;

use crate::shared::download::DownloadStatus;

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
    Http(http::StatusCode),
}

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("无效封面图")]
    InvalidCoverUrl,
    #[error("解析封面图域名失败: {0}")]
    ResolveCoverHost(#[source] std::io::Error),
    #[error("http请求失败: {0}")]
    Http(#[from] reqwest::Error),
    #[error("请求封面图响应错误: {0}")]
    HttpStatus(http::StatusCode),
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
    #[error("标签阻塞任务异常结束: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("数据库错误: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("数据库迁移错误: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("创建父目录错误: {0}")]
    CreateParent(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("请求参数无效: {0}")]
    InvalidRequest(String),
    #[error("无效的provider")]
    InvalidProvider,
    #[error("暂不支持下载该音质: {0}，当前只允许 FLAC")]
    UnsupportedDownloadQuality(String),
    #[error("任务已存在")]
    AlreadyExists,
    #[error("任务状态已变化，期望 {expected:?}，实际 {actual:?}")]
    InvalidState {
        expected: DownloadStatus,
        actual: DownloadStatus,
    },
    #[error("不允许从 {from:?} 转换到 {to:?}")]
    InvalidTransition {
        from: DownloadStatus,
        to: DownloadStatus,
    },
    #[error("Provider 操作失败: {0}")]
    Provider(#[from] ProviderError),
    #[error("下载失败: {0}")]
    Download(#[from] DownloadError),
    #[error("写入标签失败: {0}")]
    Tag(#[from] TagError),
    #[error("进度任务异常结束: {0}")]
    ProgressTask(#[from] tokio::task::JoinError),
    #[error("文件大小超过数据库 INTEGER 范围")]
    SizeOverflow,
    #[error("任务版本号超过数据库 INTEGER 范围")]
    VersionOverflow,
    #[error("数据库操作失败: {0}")]
    Database(#[from] sqlx::Error),
    #[error("任务不存在: {0}")]
    TaskNotFound(i64),
    #[error("任务状态已发生变化")]
    StateChanged,
    #[error("任务执行已中断")]
    Interrupted,
    #[error("下载文件路径丢失")]
    MissingDownloadPath,
    #[error("曲库文件路径不在下载目录中")]
    UnsafeLibraryPath,
    #[error("曲库文件操作失败: {0}")]
    LibraryIo(#[source] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("任务管理器已经停止")]
    ManagerStopped,
    #[error("数据库操作失败: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("参数校验失败")]
    Validation(#[from] garde::Report),
    #[error("请求参数错误: {0}")]
    BadRequest(String),
    #[error("资源不存在")]
    NotFound,
    #[error("当前操作不允许: {0}")]
    Conflict(String),
    #[error("服务器内部错误")]
    Service(#[from] ServiceError),
    #[error("任务管理器错误")]
    Manager(#[from] ManagerError),
}
