use std::{net::SocketAddr, sync::Arc, time::Duration};

use backend::{
    backend::Backend,
    config::{Config, load_config},
    events::DownloadEvents,
    manager::{Manager, ManagerHandle},
    mcp::MusicMcp,
    service::Service,
};
use contracts::request::PreviewRequest;
use dioxus::{
    CapturedError,
    logger::tracing,
    server::axum::{
        self, Extension,
        body::Body,
        extract::{Query, State},
        http::{HeaderMap, Request, StatusCode, header},
        middleware::{self, Next},
        response::{IntoResponse, Response},
        routing::get,
    },
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, signal};
use tokio_util::sync::CancellationToken;
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
};

use crate::App;

// 服务端组合状态，由Axum Extension注入SSR、Server Function和自定义路由。
#[derive(Clone)]
pub(crate) struct AppState {
    pub service: Arc<Service>,
    pub manager: ManagerHandle,
    pub download_events: DownloadEvents,
    pub shutdown: CancellationToken,
}

impl AppState {
    pub(crate) fn new(
        service: Arc<Service>,
        manager: ManagerHandle,
        download_events: DownloadEvents,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            service,
            manager,
            download_events,
            shutdown,
        }
    }
}

#[derive(Clone)]
struct McpAuthToken(Arc<str>);

const HTTP_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

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
    download_events: backend::events::DownloadEvents,
) -> dioxus::Result<()> {
    // HTTP、SSE、MCP 共用关闭令牌，避免长连接阻塞 Axum 优雅关闭。
    let shutdown = CancellationToken::new();
    let state = AppState::new(
        Arc::clone(&service),
        manager_handle.clone(),
        download_events,
        shutdown.clone(),
    );

    let mcp_config = StreamableHttpServerConfig::default()
        // MCP 仅在局域网开放，并由 Bearer Token 保护；允许客户端使用局域网 IP 作为 Host。
        .disable_allowed_hosts()
        .with_json_response(true)
        .with_cancellation_token(shutdown.clone());
    let mcp_service = Arc::clone(&service);
    let mcp_manager = manager_handle.clone();
    let mcp: StreamableHttpService<MusicMcp, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(MusicMcp::new(Arc::clone(&mcp_service), mcp_manager.clone())),
        Default::default(),
        mcp_config,
    );
    let authenticated_mcp = ServiceBuilder::new()
        .layer(middleware::from_fn_with_state(
            McpAuthToken(Arc::from(config.server.mcp_token.as_str())),
            require_mcp_token,
        ))
        .service(mcp);

    let request_timeout = Duration::from_secs(config.server.request_timeout);
    let http_layers = ServiceBuilder::new()
        // Request ID 必须先写入 Request extensions，TraceLayer 才能读取。
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    let request_id = request
                        .extensions()
                        .get::<RequestId>()
                        .and_then(|id| id.header_value().to_str().ok())
                        .unwrap_or("unknown");
                    tracing::info_span!(
                        "http_request",
                        request_id,
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
                .on_request(DefaultOnRequest::new().level(tracing::Level::INFO))
                .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(ConcurrencyLimitLayer::new(
            config.server.max_http_concurrent,
        ))
        // 只限制处理到响应头的时间，不会截断 SSE、MCP 或试听的响应体。
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        ))
        .layer(CatchPanicLayer::new());

    let router = dioxus::server::router(App)
        .route("/api/preview", get(stream_preview))
        .nest_service("/mcp", authenticated_mcp)
        // Dioxus Router已注册Server Function，SSR fallback和静态资源。
        // Extension会覆盖整个Router，Server Function也能提取AppState。
        .layer(Extension(state))
        .layer(http_layers);

    let address = server_address(config.server.port)?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(%address, "服务器已启动");

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

            // 同时等待两个任务完成；HTTP 长连接另有超时兜底。
            let (server_result, manager_result) = tokio::join!(
                wait_for_http_server(&mut server_task),
                &mut manager_task,
            );
            server_result?;
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
            wait_for_http_server(&mut server_task).await?;

            manager_result??;
            return Err(CapturedError::msg("下载管理器意外终止"));
        }
    }

    Ok(())
}

async fn wait_for_http_server(
    task: &mut tokio::task::JoinHandle<std::io::Result<()>>,
) -> dioxus::Result<()> {
    match tokio::time::timeout(HTTP_SHUTDOWN_TIMEOUT, &mut *task).await {
        Ok(result) => result??,
        Err(_) => {
            tracing::warn!(
                timeout_seconds = HTTP_SHUTDOWN_TIMEOUT.as_secs(),
                "HTTP 连接未在期限内结束，强制停止服务"
            );
            task.abort();
            if let Err(error) = task.await
                && !error.is_cancelled()
            {
                return Err(error.into());
            }
        }
    }
    Ok(())
}

async fn require_mcp_token(
    State(expected): State<McpAuthToken>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authorized = has_valid_mcp_token(request.headers(), &expected.0);

    if authorized {
        return next.run(request).await;
    }

    tracing::warn!("MCP 请求鉴权失败");
    let mut response = (StatusCode::UNAUTHORIZED, "MCP token 无效").into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer realm=\"tune-box-mcp\""),
    );
    response
}

fn has_valid_mcp_token(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .is_some_and(|(scheme, token)| {
            scheme.eq_ignore_ascii_case("Bearer")
                && constant_time_eq(token.as_bytes(), expected.as_bytes())
        })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
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

#[cfg(test)]
mod tests {
    use dioxus::server::axum::http::{HeaderMap, HeaderValue, header};

    use super::{constant_time_eq, has_valid_mcp_token};

    #[test]
    fn compares_mcp_tokens_without_early_byte_exit() {
        assert!(constant_time_eq(b"correct-token", b"correct-token"));
        assert!(!constant_time_eq(b"correct-token", b"wrong---token"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn accepts_only_the_configured_mcp_bearer_token() {
        let mut headers = HeaderMap::new();
        assert!(!has_valid_mcp_token(&headers, "secret-token"));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic secret-token"),
        );
        assert!(!has_valid_mcp_token(&headers, "secret-token"));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        assert!(!has_valid_mcp_token(&headers, "secret-token"));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token"),
        );
        assert!(has_valid_mcp_token(&headers, "secret-token"));
    }
}
