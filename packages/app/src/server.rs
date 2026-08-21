use std::{net::SocketAddr, sync::Arc};

use dioxus::{
    CapturedError,
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
    events::DownloadEvents,
    manager::{Manager, ManagerHandle},
    service::Service,
};
use shared::request::PreviewRequest;
use tokio::{net::TcpListener, signal};
use tokio_util::sync::CancellationToken;

use crate::App;

// 服务端组合状态，由Axum Extension注入SSR、Server Function和自定义路由。
#[derive(Clone)]
pub(crate) struct AppState {
    pub service: Arc<Service>,
    pub manager: ManagerHandle,
    pub download_events: DownloadEvents,
}

impl AppState {
    pub(crate) fn new(
        service: Arc<Service>,
        manager: ManagerHandle,
        download_events: DownloadEvents,
    ) -> Self {
        Self {
            service,
            manager,
            download_events,
        }
    }
}

pub async fn run() -> dioxus::Result<()> {
    let config = load_config("./config/config.toml").map_err(CapturedError::msg)?;
    let Backend {
        pool,
        service,
        manager,
        manager_handle,
        download_events,
    } = Backend::new(&config).await?;

    let result = serve(&config, service, manager, manager_handle, download_events).await;

    // SqlitePool::drop无法异步等待连接完全关闭，显示关闭
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
    download_events: service::events::DownloadEvents,
) -> dioxus::Result<()> {
    let state = AppState::new(service, manager_handle.clone(), download_events);

    let router = dioxus::server::router(App)
        .route("/api/preview", get(stream_preview))
        // Dioxus Router已注册Server Function，SSR fallback和静态资源。
        // Extension会覆盖整个Router，Server Function也能提取AppState。
        .layer(Extension(state));

    let address = server_address(config.server.port)?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(%address, "服务器已启动");

    // 关闭信号，异步共享
    let shutdown = CancellationToken::new();
    let server_shutdown = shutdown.clone();

    // 启动http服务任务
    let mut server_task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(server_shutdown.cancelled_owned())
            .await
    });
    // 启动manager任务
    let mut manager_task = tokio::spawn(manager.run());

    tokio::select! {
        _ = shutdown_signal() => {
            tracing::info!("收到关闭信号");

            // 停止接受新请求；Axum会继续等待当前请求完成。
            shutdown.cancel();
            // 同时通知下载管理器取消worker并保存最终状态。
            let _ = manager_handle.shutdown().await;

            // 同时等待两个任务完成
            let (server_result, manager_result) =
                tokio::join!(&mut server_task, &mut manager_task);
            server_result??;
            manager_result??;
        }
        // http服务先停止
        server_result = &mut server_task => {
            tracing::warn!("HTTP 服务已经停止");

            shutdown.cancel();
            let _ = manager_handle.shutdown().await;

            server_result??;
            manager_task.await??;
        }
        // manager任务先停止
        manager_result = &mut manager_task => {
            tracing::error!("下载管理器意外终止");

            shutdown.cancel();
            server_task.await??;

            manager_result??;
            return Err(CapturedError::msg("下载管理器意外终止"));
        }
    }

    Ok(())
}

fn server_address(configured_port: u32) -> dioxus::Result<SocketAddr> {
    // dx serve会注入内部IP/PORT，必须监听这个地址才能通过开发代理访问。
    if dioxus::cli_config::is_cli_enabled() {
        return Ok(dioxus::cli_config::fullstack_address_or_localhost());
    }

    let port = u16::try_from(configured_port)?;
    Ok(SocketAddr::from(([0, 0, 0, 0], port)))
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
    cfg_select! {
        unix => {
            use tokio::signal::unix::{SignalKind, signal as unix_signal};

            let mut terminate = unix_signal(SignalKind::terminate())
                .expect("注册SIGTERM处理器失败");

            tokio::select! {
                result = signal::ctrl_c() => {
                    if let Err(error) = result {
                        tracing::error!(%error, "监听Ctrl+C失败");
                    }
                }
                _ = terminate.recv() => {}
            }
        }
        _ => {
            if let Err(error) = signal::ctrl_c().await {
                tracing::error!(%error, "监听Ctrl+C失败");
            }
        }
    }
}
