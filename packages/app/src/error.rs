use backend::error::{ManagerError, ProviderError, ServiceError};
use dioxus::server::ServerFnError;

pub(crate) trait ServerError {
    fn into(self) -> ServerFnError;
}

impl ServerError for ServiceError {
    fn into(self) -> ServerFnError {
        let (code, message) = match &self {
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

        match code {
            502 => {
                dioxus::logger::tracing::warn!(
                    error = ?self,
                    response_code = code,
                    "上游服务请求失败"
                );
            }
            500..=599 => {
                dioxus::logger::tracing::error!(
                    error = ?self,
                    response_code = code,
                    "Server Function 执行失败"
                );
            }
            _ => {
                dioxus::logger::tracing::debug!(
                    error = %self,
                    response_code = code,
                    "Server Function 拒绝请求"
                );
            }
        }

        ServerFnError::ServerError {
            message,
            code,
            details: None,
        }
    }
}

impl ServerError for ManagerError {
    fn into(self) -> ServerFnError {
        dioxus::logger::tracing::error!(error = ?self, "下载管理器请求失败");
        ServerFnError::ServerError {
            message: "下载管理器错误".to_owned(),
            code: 500,
            details: None,
        }
    }
}
