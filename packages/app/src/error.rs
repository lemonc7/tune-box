use dioxus::server::ServerFnError;
use service::error::{ManagerError, ProviderError, ServiceError};

pub(crate) trait ServerError {
    fn into(self) -> ServerFnError;
}

impl ServerError for ServiceError {
    fn into(self) -> ServerFnError {
        let (code, message) = match self {
            ServiceError::InvalidRequest(_)
            | ServiceError::InvalidProvider
            | ServiceError::Provider(ProviderError::InvalidArgument(_)) => (400, self.to_string()),
            ServiceError::TaskNotFound(_) => (404, self.to_string()),
            ServiceError::AlreadyExists
            | ServiceError::InvalidState { .. }
            | ServiceError::InvalidTransition { .. }
            | ServiceError::StateChanged
            | ServiceError::Interrupted => (409, self.to_string()),
            ServiceError::Provider(
                ProviderError::Network(_)
                | ProviderError::Parse(_)
                | ProviderError::InvalidResponse(_),
            ) => (502, "上游音乐服务调用暂时不可用".to_owned()),
            _ => (500, "服务器内部错误".to_owned()),
        };

        ServerFnError::ServerError {
            message,
            code,
            details: None,
        }
    }
}

impl ServerError for ManagerError {
    fn into(self) -> ServerFnError {
        ServerFnError::ServerError {
            message: "下载管理器错误".to_owned(),
            code: 500,
            details: None,
        }
    }
}
