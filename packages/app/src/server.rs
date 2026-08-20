use std::{error::Error, io, net::SocketAddr, sync::Arc};

use dioxus::{
    logger::tracing,
    server::axum::{
        self, Extension,
        body::Body,
        extract::Query,
        http::{HeaderMap, StatusCode, header},
        response::{IntoResponse, Response},
        routing::get,
    },
};
use service::{
    backend::Backend,
    config::{Config, load_config},
    manager::{Manager, ManagerHandle},
    service::Service,
};
use shared::request::PreviewRequest;
use tokio::{net::TcpListener, signal};
use tokio_util::sync::CancellationToken;

use crate::{App, state::AppState};

pub type BoxError = Box<dyn Error>;

pub async fn run() -> Result<(), BoxError> {
    let config = load_config("./config/config.toml")
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let Backend {
        pool,
        service,
        manager,
        manager_handle,
    } = Backend::new(&config).await?;

    let result = serve(&config, service, manager, manager_handle).await;

    // SqlitePool::drop 无法异步等待连接完全关闭，最后显式收尾。
    tracing::info!("正在关闭数据库连接池");
    pool.close().await;
    tracing::info!("数据库连接池已关闭");

    result?;
    tracing::info!("服务器已关闭");
    Ok(())
}

async fn serve(
    config: &Config,
    service: Arc<Service>,
    manager: Manager,
    manager_handle: ManagerHandle,
) -> Result<(), BoxError> {
    let state = AppState::new(service, manager_handle.clone());

    let router = dioxus::server::router(App)
        .route("/health", get(health_check))
        .route("/api/preview", get(stream_preview))
        // Dioxus Router 已注册 Server Function、SSR fallback 和静态资源。
        // Extension 会覆盖整个 Router，Server Function 也能提取 AppState。
        .layer(Extension(state));

    let address = server_address(config.server.port)?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(%address, "服务器已启动");

    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();

    let mut server_task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(server_shutdown.cancelled_owned())
            .await
    });
    let mut manager_task = tokio::spawn(manager.run());

    tokio::select! {
        _ = shutdown_signal() => {
            tracing::info!("收到关闭信号");

            // 停止接受新请求；Axum 会继续等待当前请求完成。
            shutdown.cancel();
            // 同时通知下载管理器取消 worker 并保存最终状态。
            let _ = manager_handle.shutdown().await;

            let (server_result, manager_result) =
                tokio::join!(&mut server_task, &mut manager_task);
            server_result??;
            manager_result??;
        }
        server_result = &mut server_task => {
            tracing::warn!("HTTP 服务已经停止");

            shutdown.cancel();
            let _ = manager_handle.shutdown().await;

            server_result??;
            manager_task.await??;
        }
        manager_result = &mut manager_task => {
            tracing::error!("下载管理器意外终止");

            shutdown.cancel();
            server_task.await??;

            manager_result??;
            return Err(io::Error::other("下载管理器意外终止").into());
        }
    }

    Ok(())
}

fn server_address(configured_port: u32) -> Result<SocketAddr, BoxError> {
    // dx serve 会注入内部 IP/PORT，必须监听这个地址才能通过开发代理访问。
    if dioxus::cli_config::is_cli_enabled() {
        return Ok(dioxus::cli_config::fullstack_address_or_localhost());
    }

    let port = u16::try_from(configured_port)?;
    Ok(SocketAddr::from(([0, 0, 0, 0], port)))
}

async fn health_check(Extension(state): Extension<AppState>) -> StatusCode {
    if state.manager.is_running() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn stream_preview(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    Query(request): Query<PreviewRequest>,
) -> Response {
    if request.provider.trim().is_empty() || request.track_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "试听参数不能为空").into_response();
    }

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let upstream = match state
        .service
        .preview(&request.provider, &request.track_id, range)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "获取试听资源失败");
            return (StatusCode::BAD_GATEWAY, "试听资源暂时不可用").into_response();
        }
    };

    let status = upstream.status();
    if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
        tracing::warn!(%status, "试听资源服务器返回错误状态");
        return (StatusCode::BAD_GATEWAY, "试听资源暂时不可用").into_response();
    }

    let upstream_headers = upstream.headers().clone();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::ETAG,
        header::LAST_MODIFIED,
        header::CACHE_CONTROL,
    ] {
        if let Some(value) = upstream_headers.get(&name) {
            response.headers_mut().insert(name, value.clone());
        }
    }
    response
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal as unix_signal};

        let mut terminate = unix_signal(SignalKind::terminate()).expect("注册 SIGTERM 处理器失败");

        tokio::select! {
            result = signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(%error, "监听 Ctrl+C 失败");
                }
            }
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    if let Err(error) = signal::ctrl_c().await {
        tracing::error!(%error, "监听 Ctrl+C 失败");
    }
}
