//! HTTP代理服务器
//!
//! 基于Axum的HTTP服务器，处理代理请求
//!
//! Uses a manual hyper HTTP/1.1 accept loop with `preserve_header_case(true)` so
//! that the original header-name casing from the CLI client is captured in a
//! `HeaderCaseMap` extension.  This map is later forwarded to the upstream via
//! the hyper-based HTTP client, producing wire-level header casing identical to
//! a direct (non-proxied) CLI request.

use super::{
    failover_switch::FailoverSwitchManager,
    handlers,
    log_codes::srv as log_srv,
    provider_router::ProviderRouter,
    providers::{codex_chat_history::CodexChatHistoryStore, gemini_shadow::GeminiShadowStore},
    types::*,
    ProxyError,
};
use crate::database::Database;
use axum::{
    extract::DefaultBodyLimit,
    routing::{any, get, post},
    Router,
};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;

/// 代理服务器状态（共享）
#[derive(Clone)]
pub struct ProxyState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<ProxyConfig>>,
    pub status: Arc<RwLock<ProxyStatus>>,
    pub start_time: Arc<RwLock<Option<std::time::Instant>>>,
    /// 每个应用类型当前使用的 provider (app_type -> (provider_id, provider_name))
    pub current_providers: Arc<RwLock<std::collections::HashMap<String, (String, String)>>>,
    /// 共享的 ProviderRouter（持有熔断器状态，跨请求保持）
    pub provider_router: Arc<ProviderRouter>,
    /// Gemini Native shadow state，用于 thoughtSignature / tool call 回放
    pub gemini_shadow: Arc<GeminiShadowStore>,
    /// Codex Chat bridge history，用于恢复 previous_response_id 指向的 tool call
    pub codex_chat_history: Arc<CodexChatHistoryStore>,
    /// AppHandle，用于发射事件和更新托盘菜单
    pub app_handle: Option<tauri::AppHandle>,
    /// 故障转移切换管理器
    pub failover_manager: Arc<FailoverSwitchManager>,
}

/// 代理HTTP服务器
pub struct ProxyServer {
    config: ProxyConfig,
    state: ProxyState,
    shutdown_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
    /// 服务器任务句柄，用于等待服务器实际关闭
    server_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl ProxyServer {
    pub fn new(
        config: ProxyConfig,
        db: Arc<Database>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        // 创建共享的 ProviderRouter（熔断器状态将跨所有请求保持）
        let provider_router = Arc::new(ProviderRouter::new(db.clone()));
        // 创建故障转移切换管理器
        let failover_manager = Arc::new(FailoverSwitchManager::new(db.clone()));

        let state = ProxyState {
            db,
            config: Arc::new(RwLock::new(config.clone())),
            status: Arc::new(RwLock::new(ProxyStatus::default())),
            start_time: Arc::new(RwLock::new(None)),
            current_providers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            provider_router,
            gemini_shadow: Arc::new(GeminiShadowStore::default()),
            codex_chat_history: Arc::new(CodexChatHistoryStore::default()),
            app_handle,
            failover_manager,
        };

        Self {
            config,
            state,
            shutdown_tx: Arc::new(RwLock::new(None)),
            server_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<ProxyServerInfo, ProxyError> {
        // 检查是否已在运行
        if self.shutdown_tx.read().await.is_some() {
            return Err(ProxyError::AlreadyRunning);
        }

        let addr: SocketAddr =
            format!("{}:{}", self.config.listen_address, self.config.listen_port)
                .parse()
                .map_err(|e| ProxyError::BindFailed(format!("无效的地址: {e}")))?;

        // 创建关闭通道
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // 构建路由
        let app = self.build_router();

        // 绑定监听器
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| ProxyError::BindFailed(e.to_string()))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| ProxyError::BindFailed(e.to_string()))?;
        let actual_port = local_addr.port();

        log::info!("[{}] 代理服务器启动于 {local_addr}", log_srv::STARTED);

        // 更新全局代理端口，用于系统代理检测
        crate::proxy::http_client::set_proxy_port(actual_port);

        // 保存关闭句柄
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        // 更新状态
        let mut status = self.state.status.write().await;
        status.running = true;
        status.address = self.config.listen_address.clone();
        status.port = actual_port;
        drop(status);

        // 记录启动时间
        *self.state.start_time.write().await = Some(std::time::Instant::now());

        // 启动服务器 — 使用手动 hyper HTTP/1.1 accept loop
        // 开启 preserve_header_case 以捕获客户端请求头的原始大小写
        let state = self.state.clone();
        let handle = tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let (stream, _remote_addr) = match result {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("[{SRV}] accept 失败: {e}", SRV = log_srv::ACCEPT_ERR);
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                continue;
                            }
                        };

                        let app = app.clone();
                        tokio::spawn(async move {
                            // Peek raw TCP bytes to capture original header casing
                            // before hyper parses (and lowercases) the header names.
                            let original_cases = {
                                let mut peek_buf = vec![0u8; 8192];
                                match stream.peek(&mut peek_buf).await {
                                    Ok(n) => {
                                        let cases = super::hyper_client::OriginalHeaderCases::from_raw_bytes(&peek_buf[..n]);
                                        log::debug!(
                                            "[ProxyServer] Peeked {} bytes, captured {} header casings",
                                            n, cases.cases.len()
                                        );
                                        cases
                                    }
                                    Err(e) => {
                                        log::debug!("[ProxyServer] peek failed (non-fatal): {e}");
                                        super::hyper_client::OriginalHeaderCases::default()
                                    }
                                }
                            };

                            // service_fn 将 axum Router（tower::Service）桥接到 hyper
                            let service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                                let mut router = app.clone();
                                let cases = original_cases.clone();
                                async move {
                                    // 将 hyper::body::Incoming 转为 axum::body::Body，保留 extensions
                                    let (mut parts, body) = req.into_parts();

                                    // Insert our own header case map alongside hyper's internal one
                                    parts.extensions.insert(cases);

                                    let body = axum::body::Body::new(body);
                                    let axum_req = http::Request::from_parts(parts, body);
                                    <Router as tower::Service<http::Request<axum::body::Body>>>::call(&mut router, axum_req).await
                                }
                            });

                            if let Err(e) = hyper::server::conn::http1::Builder::new()
                                .preserve_header_case(true)
                                .serve_connection(TokioIo::new(stream), service)
                                .await
                            {
                                // Connection reset / broken pipe 等在代理场景下很常见，debug 级别
                                log::debug!("[{SRV}] connection error: {e}", SRV = log_srv::CONN_ERR);
                            }
                        });
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }

            // 服务器停止后更新状态
            state.status.write().await.running = false;
            *state.start_time.write().await = None;
        });

        // 保存服务器任务句柄
        *self.server_handle.write().await = Some(handle);

        Ok(ProxyServerInfo {
            address: self.config.listen_address.clone(),
            port: actual_port,
            started_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn stop(&self) -> Result<(), ProxyError> {
        // 1. 发送关闭信号
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        } else {
            return Err(ProxyError::NotRunning);
        }

        // 2. 等待服务器任务结束（带 5 秒超时保护）
        if let Some(handle) = self.server_handle.write().await.take() {
            match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                Ok(Ok(())) => {
                    log::info!("[{}] 代理服务器已完全停止", log_srv::STOPPED);
                    Ok(())
                }
                Ok(Err(e)) => {
                    log::warn!("[{}] 代理服务器任务异常终止: {e}", log_srv::TASK_ERROR);
                    Err(ProxyError::StopFailed(e.to_string()))
                }
                Err(_) => {
                    log::warn!(
                        "[{}] 代理服务器停止超时（5秒），强制继续",
                        log_srv::STOP_TIMEOUT
                    );
                    Err(ProxyError::StopTimeout)
                }
            }
        } else {
            Ok(())
        }
    }

    pub async fn get_status(&self) -> ProxyStatus {
        let mut status = self.state.status.read().await.clone();

        // 计算运行时间
        if let Some(start) = *self.state.start_time.read().await {
            status.uptime_seconds = start.elapsed().as_secs();
        }

        // 从 current_providers HashMap 获取每个应用类型当前正在使用的 provider
        let current_providers = self.state.current_providers.read().await;
        status.active_targets = current_providers
            .iter()
            .map(|(app_type, (provider_id, provider_name))| ActiveTarget {
                app_type: app_type.clone(),
                provider_id: provider_id.clone(),
                provider_name: provider_name.clone(),
            })
            .collect();

        status
    }

    /// 更新某个应用类型当前“目标供应商”（用于 UI 展示 active_targets）
    ///
    /// 注意：这不代表该供应商一定已经处理过请求，而是用于“热切换/启用故障转移立即切 P1”
    /// 等场景下，让 UI 能立刻反映最新目标。
    pub async fn set_active_target(&self, app_type: &str, provider_id: &str, provider_name: &str) {
        let mut current_providers = self.state.current_providers.write().await;
        current_providers.insert(
            app_type.to_string(),
            (provider_id.to_string(), provider_name.to_string()),
        );
    }

    fn build_router(&self) -> Router {
        Router::new()
            // 健康检查
            .route("/health", get(handlers::health_check))
            .route("/status", get(handlers::get_status))
            // Claude API (支持带前缀和不带前缀两种格式)
            .route("/v1/messages", post(handlers::handle_messages))
            .route("/claude/v1/messages", post(handlers::handle_messages))
            // Claude Desktop 3P 本地 gateway（独立 provider namespace）
            .route(
                "/claude-desktop/v1/models",
                get(handlers::handle_claude_desktop_models),
            )
            .route(
                "/claude-desktop/v1/messages",
                post(handlers::handle_claude_desktop_messages),
            )
            // OpenAI Chat Completions API (Codex CLI，支持带前缀和不带前缀)
            .route("/chat/completions", post(handlers::handle_chat_completions))
            .route(
                "/v1/chat/completions",
                post(handlers::handle_chat_completions),
            )
            .route(
                "/v1/v1/chat/completions",
                post(handlers::handle_chat_completions),
            )
            .route(
                "/codex/v1/chat/completions",
                post(handlers::handle_chat_completions),
            )
            // OpenAI Models API (Codex CLI reachability check)
            .route("/models", get(handlers::handle_models))
            .route("/v1/models", get(handlers::handle_models))
            // OpenAI Responses API (Codex CLI，支持带前缀和不带前缀)
            .route("/responses", post(handlers::handle_responses))
            .route("/v1/responses", post(handlers::handle_responses))
            .route("/v1/v1/responses", post(handlers::handle_responses))
            .route("/codex/v1/responses", post(handlers::handle_responses))
            // Grok Build uses the Responses protocol but has an independent
            // provider namespace and failover queue.
            .route(
                "/grokbuild/v1/responses",
                post(handlers::handle_grokbuild_responses),
            )
            // OpenAI Responses Compact API (Codex CLI 远程压缩，透传)
            .route(
                "/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/v1/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/v1/v1/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/codex/v1/responses/compact",
                post(handlers::handle_responses_compact),
            )
            .route(
                "/grokbuild/v1/responses/compact",
                post(handlers::handle_grokbuild_responses_compact),
            )
            // Codex standalone Alpha Search API. All local aliases normalize to
            // the selected provider's canonical sibling `/alpha/search` route.
            .route("/alpha/search", post(handlers::handle_alpha_search))
            .route("/v1/alpha/search", post(handlers::handle_alpha_search))
            .route("/v1/v1/alpha/search", post(handlers::handle_alpha_search))
            .route(
                "/codex/v1/alpha/search",
                post(handlers::handle_alpha_search),
            )
            // Codex built-in ImageGen still calls the legacy OpenAI Images API.
            .route(
                "/images/generations",
                post(handlers::handle_images_generations),
            )
            .route(
                "/v1/images/generations",
                post(handlers::handle_images_generations),
            )
            .route(
                "/v1/v1/images/generations",
                post(handlers::handle_images_generations),
            )
            .route(
                "/codex/v1/images/generations",
                post(handlers::handle_images_generations),
            )
            // Codex ImageGen posts to `/images/edits` when it references existing images.
            .route("/images/edits", post(handlers::handle_images_edits))
            .route("/v1/images/edits", post(handlers::handle_images_edits))
            .route("/v1/v1/images/edits", post(handlers::handle_images_edits))
            .route(
                "/codex/v1/images/edits",
                post(handlers::handle_images_edits),
            )
            // Gemini API (支持带前缀和不带前缀)
            //
            // 用 `any(..)` 覆盖所有 HTTP 方法：除了 POST `:generateContent` /
            // `:streamGenerateContent` / `:countTokens` 之外，Gemini SDK / CLI 还会发
            // GET `/models`、GET `/models/<id>` 等只读端点。如果只挂 POST，这些 GET
            // 请求会在路由层 404，绕过本地代理的统计、整流和故障转移。
            .route("/v1beta/*path", any(handlers::handle_gemini))
            .route("/gemini/v1beta/*path", any(handlers::handle_gemini))
            // Gemini 的 GA 版本也叫 /v1，给原 SDK 留一条出口
            .route("/gemini/v1/*path", any(handlers::handle_gemini))
            // 提高默认请求体大小限制（避免 413 Payload Too Large）
            .layer(DefaultBodyLimit::max(200 * 1024 * 1024))
            .with_state(self.state.clone())
    }

    /// 在不重启服务的情况下更新运行时配置
    pub async fn apply_runtime_config(&self, config: &ProxyConfig) {
        *self.state.config.write().await = config.clone();
    }

    /// 热更新熔断器配置
    ///
    /// 将新配置应用到所有已创建的熔断器实例
    pub async fn update_circuit_breaker_configs(
        &self,
        config: super::circuit_breaker::CircuitBreakerConfig,
    ) {
        self.state.provider_router.update_all_configs(config).await;
    }

    pub async fn update_circuit_breaker_config_for_app(
        &self,
        app_type: &str,
        config: super::circuit_breaker::CircuitBreakerConfig,
    ) {
        self.state
            .provider_router
            .update_app_configs(app_type, config)
            .await;
    }

    /// 重置指定 Provider 的熔断器
    pub async fn reset_provider_circuit_breaker(&self, provider_id: &str, app_type: &str) {
        self.state
            .provider_router
            .reset_provider_breaker(provider_id, app_type)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Provider, ProviderMeta};
    use crate::AppError;
    use axum::http::{header, HeaderMap, StatusCode};
    use serde_json::{json, Value};
    use tokio::sync::Mutex;

    #[derive(Debug)]
    struct CapturedRequest {
        path_and_query: String,
        authorization: Option<String>,
        body: Value,
    }

    /// mock 上游对 Chat 路径的固定回复（`chat.completion` 形状）。放在这里而不是
    /// 逐个调用点内联：形状只有一份定义，Chat 转换器的入参契约改动时不会漏改。
    const CODEX_CHAT_COMPLETION_BODY: &str = r#"{"id":"chatcmpl_1","object":"chat.completion","created":1,"model":"kimi-k2","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#;

    /// A base URL pasted as a complete endpoint with the full-URL switch left off
    /// must derive the sibling standalone endpoint instead of having the
    /// standalone path appended to it.
    #[tokio::test]
    async fn codex_standalone_endpoints_derive_from_pasted_full_base_url() {
        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let capture_handler = {
            let captured = captured.clone();
            move |request: axum::extract::Request| {
                let captured = captured.clone();
                async move {
                    let (parts, _body) = request.into_parts();
                    captured.lock().await.push(CapturedRequest {
                        path_and_query: parts
                            .uri
                            .path_and_query()
                            .map(|value| value.as_str().to_string())
                            .unwrap_or_else(|| parts.uri.path().to_string()),
                        authorization: parts
                            .headers
                            .get(header::AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(ToString::to_string),
                        body: Value::Null,
                    });

                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        r#"{"created":1,"data":[{"b64_json":"aW1hZ2U="}],"usage":{"input_tokens":7,"output_tokens":11,"total_tokens":18}}"#,
                    )
                }
            }
        };
        let mock_app = Router::new()
            .route("/v1/images/generations", post(capture_handler.clone()))
            .route("/v1/images/edits", post(capture_handler.clone()))
            .route("/Gateway/v1/images/edits", post(capture_handler.clone()))
            .route("/v1/alpha/search", post(capture_handler));
        let mock_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock upstream");
        let mock_addr = mock_listener.local_addr().expect("mock upstream address");
        let mock_handle = tokio::spawn(async move {
            axum::serve(mock_listener, mock_app)
                .await
                .expect("serve mock upstream");
        });

        let db = Arc::new(Database::memory().expect("memory database"));
        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();

        let cases = [
            (
                "pasted-mixed-case-chat-completions",
                format!("http://{mock_addr}/v1/Chat/Completions?api-version=CaseValue"),
                "/v1/images/edits",
                "/v1/images/edits?api-version=CaseValue&client_version=0.145.0",
            ),
            (
                "pasted-mixed-case-images-generations",
                format!("http://{mock_addr}/Gateway/v1/Images/Generations/?api-version=CaseValue#fragment"),
                "/v1/images/edits",
                "/Gateway/v1/images/edits?api-version=CaseValue&client_version=0.145.0",
            ),
            (
                "pasted-mixed-case-images-edits",
                format!("http://{mock_addr}/v1/Images/Edits/"),
                "/v1/images/generations",
                "/v1/images/generations?client_version=0.145.0",
            ),
            (
                "pasted-mixed-case-responses-compact",
                format!("http://{mock_addr}/v1/Responses/Compact/"),
                "/v1/alpha/search",
                "/v1/alpha/search?client_version=0.145.0",
            ),
            (
                "pasted-chat-completions",
                format!("http://{mock_addr}/v1/chat/completions"),
                "/v1/images/generations",
                "/v1/images/generations?client_version=0.145.0",
            ),
            (
                "pasted-chat-completions",
                format!("http://{mock_addr}/v1/chat/completions"),
                "/v1/images/edits",
                "/v1/images/edits?client_version=0.145.0",
            ),
            (
                "pasted-images-generations",
                format!("http://{mock_addr}/v1/images/generations?api-version=test"),
                "/v1/images/edits",
                "/v1/images/edits?api-version=test&client_version=0.145.0",
            ),
            (
                "pasted-responses",
                format!("http://{mock_addr}/v1/responses"),
                "/v1/alpha/search",
                "/v1/alpha/search?client_version=0.145.0",
            ),
        ];

        for (provider_id, base_url, local_path, expected_upstream) in cases {
            let provider = Provider::with_id(
                provider_id.to_string(),
                provider_id.to_string(),
                json!({
                    "base_url": base_url,
                    "auth": {"OPENAI_API_KEY": "upstream-secret"}
                }),
                None,
            );
            db.save_provider("codex", &provider)
                .expect("save pasted base URL provider");
            db.set_current_provider("codex", &provider.id)
                .expect("select pasted base URL provider");

            let response = client
                .post(format!(
                    "http://127.0.0.1:{}{local_path}?client_version=0.145.0",
                    proxy_info.port
                ))
                .header(header::AUTHORIZATION, "Bearer client-secret")
                .json(&json!({"model": "gpt-image-1", "prompt": "pasted base URL"}))
                .send()
                .await
                .expect("send images request");

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{local_path} via {base_url}"
            );
            let request = captured
                .lock()
                .await
                .pop()
                .expect("upstream request captured");
            assert_eq!(
                request.path_and_query, expected_upstream,
                "{local_path} via {base_url}"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer upstream-secret")
            );
        }

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }

    #[tokio::test]
    async fn codex_images_generation_aliases_forward_and_record_usage() {
        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let mock_app = Router::new().route(
            "/v1/images/generations",
            post({
                let captured = captured.clone();
                move |request: axum::extract::Request| {
                    let captured = captured.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = axum::body::to_bytes(body, 1024 * 1024)
                            .await
                            .expect("read mock request body");
                        captured.lock().await.push(CapturedRequest {
                            path_and_query: parts
                                .uri
                                .path_and_query()
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parts.uri.path().to_string()),
                            authorization: parts
                                .headers
                                .get(header::AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(ToString::to_string),
                            body: serde_json::from_slice(&body).expect("parse mock request body"),
                        });

                        (
                            StatusCode::OK,
                            [(header::CONTENT_TYPE, "application/json")],
                            r#"{"created":1,"data":[{"b64_json":"aW1hZ2U="}],"usage":{"input_tokens":7,"output_tokens":11,"total_tokens":18}}"#,
                        )
                    }
                }
            }),
        );
        let mock_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock upstream");
        let mock_addr = mock_listener.local_addr().expect("mock upstream address");
        let mock_handle = tokio::spawn(async move {
            axum::serve(mock_listener, mock_app)
                .await
                .expect("serve mock upstream");
        });

        let db = Arc::new(Database::memory().expect("memory database"));
        let provider = Provider::with_id(
            "images-api-upstream".to_string(),
            "Images API Upstream".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "upstream-secret"}
            }),
            None,
        );
        db.save_provider("codex", &provider)
            .expect("save test provider");
        db.set_current_provider("codex", &provider.id)
            .expect("select test provider");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: true,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();
        let aliases = [
            "/images/generations",
            "/v1/images/generations",
            "/v1/v1/images/generations",
            "/codex/v1/images/generations",
        ];

        for (index, path) in aliases.iter().enumerate() {
            let response = client
                .post(format!(
                    "http://127.0.0.1:{}{}?client_version=0.145.0",
                    proxy_info.port, path
                ))
                .header(header::AUTHORIZATION, "Bearer client-secret")
                .json(&json!({
                    "model": "gpt-image-1",
                    "prompt": format!("image generation alias {index}")
                }))
                .send()
                .await
                .expect("send images request");

            assert_eq!(response.status(), StatusCode::OK, "alias {path}");
            assert_eq!(
                response.text().await.expect("read proxy response"),
                r#"{"created":1,"data":[{"b64_json":"aW1hZ2U="}],"usage":{"input_tokens":7,"output_tokens":11,"total_tokens":18}}"#,
                "alias {path}"
            );
        }

        let (log_count, input_tokens, output_tokens): (i64, i64, i64) =
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    let totals = {
                        let conn = crate::database::lock_conn!(db.conn);
                        match conn.query_row(
                            "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
                             FROM proxy_request_logs WHERE provider_id = ?1",
                            ["images-api-upstream"],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        ) {
                            Ok(totals) => totals,
                            Err(error) => panic!("query images usage logs: {error}"),
                        }
                    };

                    if totals.0 == aliases.len() as i64 {
                        break Ok::<_, AppError>(totals);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("images usage logs were not recorded")
            .expect("read images usage logs");
        assert_eq!(log_count, aliases.len() as i64);
        assert_eq!(input_tokens, 7 * aliases.len() as i64);
        assert_eq!(output_tokens, 11 * aliases.len() as i64);

        let mut full_url_provider = Provider::with_id(
            "images-api-full-url".to_string(),
            "Images API Full URL".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1/responses?api-version=test"),
                "auth": {"OPENAI_API_KEY": "full-url-secret"}
            }),
            None,
        );
        full_url_provider.meta = Some(ProviderMeta {
            is_full_url: Some(true),
            ..ProviderMeta::default()
        });
        db.save_provider("codex", &full_url_provider)
            .expect("save full URL images provider");
        db.set_current_provider("codex", &full_url_provider.id)
            .expect("select full URL images provider");

        let response = client
            .post(format!(
                "http://127.0.0.1:{}/v1/images/generations?client_version=0.145.0",
                proxy_info.port
            ))
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&json!({
                "model": "gpt-image-1",
                "prompt": "image generation full URL"
            }))
            .send()
            .await
            .expect("send full URL images request");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.text().await.expect("read full URL image response"),
            r#"{"created":1,"data":[{"b64_json":"aW1hZ2U="}],"usage":{"input_tokens":7,"output_tokens":11,"total_tokens":18}}"#
        );

        let captured = captured.lock().await;
        assert_eq!(captured.len(), aliases.len() + 1);
        for (index, request) in captured.iter().take(aliases.len()).enumerate() {
            assert_eq!(
                request.path_and_query,
                "/v1/images/generations?client_version=0.145.0"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer upstream-secret")
            );
            assert_eq!(request.body["model"], "gpt-image-1");
            assert_eq!(
                request.body["prompt"],
                format!("image generation alias {index}")
            );
        }

        let full_url_request = captured.last().expect("full URL image request captured");
        assert_eq!(
            full_url_request.path_and_query,
            "/v1/images/generations?api-version=test&client_version=0.145.0"
        );
        assert_eq!(
            full_url_request.authorization.as_deref(),
            Some("Bearer full-url-secret")
        );
        assert_eq!(full_url_request.body["model"], "gpt-image-1");
        assert_eq!(full_url_request.body["prompt"], "image generation full URL");

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }

    #[tokio::test]
    async fn codex_images_edit_aliases_forward_and_record_usage() {
        // Real Images API responses carry `input_tokens_details` with text/image
        // splits; the shared Codex usage parser must tolerate them.
        const UPSTREAM_BODY: &str = r#"{"created":1,"data":[{"b64_json":"ZWRpdA=="}],"usage":{"input_tokens":13,"output_tokens":17,"total_tokens":30,"input_tokens_details":{"text_tokens":5,"image_tokens":8}}}"#;
        const IMAGE_DATA_URL: &str = "data:image/png;base64,Zm9v";

        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let mock_app = Router::new().route(
            "/v1/images/edits",
            post({
                let captured = captured.clone();
                move |request: axum::extract::Request| {
                    let captured = captured.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = axum::body::to_bytes(body, 1024 * 1024)
                            .await
                            .expect("read mock request body");
                        captured.lock().await.push(CapturedRequest {
                            path_and_query: parts
                                .uri
                                .path_and_query()
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parts.uri.path().to_string()),
                            authorization: parts
                                .headers
                                .get(header::AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(ToString::to_string),
                            body: serde_json::from_slice(&body).expect("parse mock request body"),
                        });

                        (
                            StatusCode::OK,
                            [(header::CONTENT_TYPE, "application/json")],
                            UPSTREAM_BODY,
                        )
                    }
                }
            }),
        );
        let mock_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock upstream");
        let mock_addr = mock_listener.local_addr().expect("mock upstream address");
        let mock_handle = tokio::spawn(async move {
            axum::serve(mock_listener, mock_app)
                .await
                .expect("serve mock upstream");
        });

        let db = Arc::new(Database::memory().expect("memory database"));
        let provider = Provider::with_id(
            "images-edit-upstream".to_string(),
            "Images Edit Upstream".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "upstream-secret"}
            }),
            None,
        );
        db.save_provider("codex", &provider)
            .expect("save test provider");
        db.set_current_provider("codex", &provider.id)
            .expect("select test provider");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: true,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();
        let aliases = [
            "/images/edits",
            "/v1/images/edits",
            "/v1/v1/images/edits",
            "/codex/v1/images/edits",
        ];

        for (index, path) in aliases.iter().enumerate() {
            let response = client
                .post(format!(
                    "http://127.0.0.1:{}{}?client_version=0.145.0",
                    proxy_info.port, path
                ))
                .header(header::AUTHORIZATION, "Bearer client-secret")
                .json(&json!({
                    "model": "gpt-image-2",
                    "prompt": format!("image edit alias {index}"),
                    "images": [{"image_url": IMAGE_DATA_URL}]
                }))
                .send()
                .await
                .expect("send images edit request");

            assert_eq!(response.status(), StatusCode::OK, "alias {path}");
            assert_eq!(
                response.text().await.expect("read proxy response"),
                UPSTREAM_BODY,
                "alias {path}"
            );
        }

        let (log_count, input_tokens, output_tokens): (i64, i64, i64) =
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    let totals = {
                        let conn = crate::database::lock_conn!(db.conn);
                        match conn.query_row(
                            "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
                             FROM proxy_request_logs WHERE provider_id = ?1",
                            ["images-edit-upstream"],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        ) {
                            Ok(totals) => totals,
                            Err(error) => panic!("query images edit usage logs: {error}"),
                        }
                    };

                    if totals.0 == aliases.len() as i64 {
                        break Ok::<_, AppError>(totals);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("images edit usage logs were not recorded")
            .expect("read images edit usage logs");
        assert_eq!(log_count, aliases.len() as i64);
        assert_eq!(input_tokens, 13 * aliases.len() as i64);
        assert_eq!(output_tokens, 17 * aliases.len() as i64);

        // A full URL pasted for the sibling generations route must derive
        // `/images/edits` instead of posting edit payloads to generations.
        let mut full_url_provider = Provider::with_id(
            "images-edit-full-url".to_string(),
            "Images Edit Full URL".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1/images/generations?api-version=test"),
                "auth": {"OPENAI_API_KEY": "full-url-secret"}
            }),
            None,
        );
        full_url_provider.meta = Some(ProviderMeta {
            is_full_url: Some(true),
            ..ProviderMeta::default()
        });
        db.save_provider("codex", &full_url_provider)
            .expect("save full URL images edit provider");
        db.set_current_provider("codex", &full_url_provider.id)
            .expect("select full URL images edit provider");

        let response = client
            .post(format!(
                "http://127.0.0.1:{}/v1/images/edits?client_version=0.145.0",
                proxy_info.port
            ))
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&json!({
                "model": "gpt-image-2",
                "prompt": "image edit full URL",
                "images": [{"image_url": IMAGE_DATA_URL}]
            }))
            .send()
            .await
            .expect("send full URL images edit request");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .text()
                .await
                .expect("read full URL image edit response"),
            UPSTREAM_BODY
        );

        let captured = captured.lock().await;
        assert_eq!(captured.len(), aliases.len() + 1);
        for (index, request) in captured.iter().take(aliases.len()).enumerate() {
            assert_eq!(
                request.path_and_query,
                "/v1/images/edits?client_version=0.145.0"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer upstream-secret")
            );
            assert_eq!(request.body["model"], "gpt-image-2");
            assert_eq!(request.body["prompt"], format!("image edit alias {index}"));
            assert_eq!(request.body["images"][0]["image_url"], IMAGE_DATA_URL);
        }

        let full_url_request = captured
            .last()
            .expect("full URL image edit request captured");
        assert_eq!(
            full_url_request.path_and_query,
            "/v1/images/edits?api-version=test&client_version=0.145.0"
        );
        assert_eq!(
            full_url_request.authorization.as_deref(),
            Some("Bearer full-url-secret")
        );
        assert_eq!(full_url_request.body["model"], "gpt-image-2");
        assert_eq!(full_url_request.body["prompt"], "image edit full URL");
        assert_eq!(
            full_url_request.body["images"][0]["image_url"],
            IMAGE_DATA_URL
        );

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }

    #[tokio::test]
    async fn alpha_search_routes_forward_to_canonical_upstream() {
        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let mock_app = Router::new().route(
            "/v1/alpha/search",
            post({
                let captured = captured.clone();
                move |request: axum::extract::Request| {
                    let captured = captured.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = axum::body::to_bytes(body, 1024 * 1024)
                            .await
                            .expect("read mock request body");
                        captured.lock().await.push(CapturedRequest {
                            path_and_query: parts
                                .uri
                                .path_and_query()
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parts.uri.path().to_string()),
                            authorization: parts
                                .headers
                                .get(header::AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(ToString::to_string),
                            body: serde_json::from_slice(&body).expect("parse mock request body"),
                        });

                        let mut headers = HeaderMap::new();
                        headers.insert(
                            header::CONTENT_TYPE,
                            "application/json".parse().expect("content type"),
                        );
                        headers.insert(
                            "x-upstream-request-id",
                            "search-1".parse().expect("request id"),
                        );
                        (
                            StatusCode::ACCEPTED,
                            headers,
                            r#"{"encrypted_output":"ciphertext"}"#,
                        )
                    }
                }
            }),
        );
        let mock_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock upstream");
        let mock_addr = mock_listener.local_addr().expect("mock upstream address");
        let mock_handle = tokio::spawn(async move {
            axum::serve(mock_listener, mock_app)
                .await
                .expect("serve mock upstream");
        });

        let db = Arc::new(Database::memory().expect("memory database"));
        let provider = Provider::with_id(
            "alpha-search-upstream".to_string(),
            "Alpha Search Upstream".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "upstream-secret"}
            }),
            None,
        );
        db.save_provider("codex", &provider)
            .expect("save test provider");
        db.set_current_provider("codex", &provider.id)
            .expect("select test provider");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: false,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();
        let aliases = [
            "/alpha/search",
            "/v1/alpha/search",
            "/v1/v1/alpha/search",
            "/codex/v1/alpha/search",
        ];

        for (index, path) in aliases.iter().enumerate() {
            let response = client
                .post(format!(
                    "http://127.0.0.1:{}{}?client_version=0.144.6",
                    proxy_info.port, path
                ))
                .header(header::AUTHORIZATION, "Bearer client-secret")
                .json(&json!({
                    "id": format!("search-{index}"),
                    "model": "gpt-5.6-sol",
                    "commands": {"search_query": [{"q": "test"}]}
                }))
                .send()
                .await
                .expect("send alpha search request");

            assert_eq!(response.status(), StatusCode::ACCEPTED, "alias {path}");
            assert_eq!(
                response
                    .headers()
                    .get("x-upstream-request-id")
                    .and_then(|value| value.to_str().ok()),
                Some("search-1"),
                "alias {path}"
            );
            assert_eq!(
                response.text().await.expect("read proxy response"),
                r#"{"encrypted_output":"ciphertext"}"#,
                "alias {path}"
            );
        }

        // Full-URL providers were the known flaw in the original PR: without a
        // sibling-endpoint rewrite, this request would be posted back to
        // `/v1/responses` instead of `/v1/alpha/search`.
        let mut full_url_provider = Provider::with_id(
            "alpha-search-full-url".to_string(),
            "Alpha Search Full URL".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1/responses?api-version=test"),
                "auth": {"OPENAI_API_KEY": "full-url-secret"}
            }),
            None,
        );
        full_url_provider.meta = Some(ProviderMeta {
            is_full_url: Some(true),
            ..ProviderMeta::default()
        });
        db.save_provider("codex", &full_url_provider)
            .expect("save full URL provider");
        db.set_current_provider("codex", &full_url_provider.id)
            .expect("select full URL provider");

        let response = client
            .post(format!(
                "http://127.0.0.1:{}/v1/alpha/search?client_version=0.144.6",
                proxy_info.port
            ))
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&json!({
                "id": "search-full-url",
                "model": "gpt-5.6-sol",
                "commands": {"search_query": [{"q": "full URL"}]}
            }))
            .send()
            .await
            .expect("send full URL alpha search request");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(
            response.text().await.expect("read full URL response"),
            r#"{"encrypted_output":"ciphertext"}"#
        );

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();

        let captured = captured.lock().await;
        assert_eq!(captured.len(), aliases.len() + 1);
        for (index, request) in captured.iter().take(aliases.len()).enumerate() {
            assert_eq!(
                request.path_and_query,
                "/v1/alpha/search?client_version=0.144.6"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer upstream-secret")
            );
            assert_eq!(request.body["id"], format!("search-{index}"));
            assert_eq!(request.body["model"], "gpt-5.6-sol");
            assert_eq!(request.body["commands"]["search_query"][0]["q"], "test");
        }

        let full_url_request = captured.last().expect("full URL request captured");
        assert_eq!(
            full_url_request.path_and_query,
            "/v1/alpha/search?api-version=test&client_version=0.144.6"
        );
        assert_eq!(
            full_url_request.authorization.as_deref(),
            Some("Bearer full-url-secret")
        );
        assert_eq!(full_url_request.body["id"], "search-full-url");
        assert_eq!(
            full_url_request.body["commands"]["search_query"][0]["q"],
            "full URL"
        );
    }

    /// 端到端：聚合供应商按请求模型把请求路由到槽位目标供应商，并把槽位的上游模型名
    /// 写进出站 body（本任务的验收核心）。用真实 `ProxyServer` + mock 上游驱动
    /// `/claude-desktop/v1/messages`，断言 mock 上游**实际收到**的 body 与鉴权。
    #[tokio::test]
    async fn aggregate_routes_rewrite_model_and_use_target_provider() {
        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let mock_app = Router::new().route(
            "/v1/messages",
            post({
                let captured = captured.clone();
                move |request: axum::extract::Request| {
                    let captured = captured.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = axum::body::to_bytes(body, 1024 * 1024)
                            .await
                            .expect("read mock request body");
                        captured.lock().await.push(CapturedRequest {
                            path_and_query: parts
                                .uri
                                .path_and_query()
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parts.uri.path().to_string()),
                            authorization: parts
                                .headers
                                .get(header::AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(ToString::to_string),
                            body: serde_json::from_slice(&body).expect("parse mock request body"),
                        });

                        (
                            StatusCode::OK,
                            [(header::CONTENT_TYPE, "application/json")],
                            r#"{"id":"msg_1","type":"message","role":"assistant","model":"glm-5.3","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":3,"output_tokens":2}}"#,
                        )
                    }
                }
            }),
        );
        let mock_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock upstream");
        let mock_addr = mock_listener.local_addr().expect("mock upstream address");
        let mock_handle = tokio::spawn(async move {
            axum::serve(mock_listener, mock_app)
                .await
                .expect("serve mock upstream");
        });

        let db = Arc::new(Database::memory().expect("memory database"));

        // 槽位目标：端点与凭据都来自它自己
        let glm_target = Provider::with_id(
            "glm-target".to_string(),
            "GLM Target".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": format!("http://{mock_addr}"),
                    "ANTHROPIC_AUTH_TOKEN": "target-secret"
                }
            }),
            None,
        );
        db.save_provider("claude-desktop", &glm_target)
            .expect("save slot target");

        // 默认目标：未命中路由时兜底。自带一条 route（映射到同名上游），
        // 用于证明「未命中不套用槽位的上游模型名」。
        let mut default_target = Provider::with_id(
            "default-target".to_string(),
            "Default Target".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": format!("http://{mock_addr}"),
                    "ANTHROPIC_AUTH_TOKEN": "default-secret"
                }
            }),
            None,
        );
        default_target.meta = Some(ProviderMeta {
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-haiku-4-5".to_string(),
                crate::provider::ClaudeDesktopModelRoute {
                    model: "claude-haiku-4-5".to_string(),
                    label_override: None,
                    supports_1m: None,
                },
            )]),
            ..Default::default()
        });
        db.save_provider("claude-desktop", &default_target)
            .expect("save default target");

        // 聚合供应商：无端点无凭据，只有一张路由表
        let mut aggregate = Provider::with_id(
            "agg".to_string(),
            "Aggregate".to_string(),
            json!({ "env": {} }),
            None,
        );
        aggregate.meta = Some(ProviderMeta {
            aggregate_routes: Some(crate::aggregate::AggregateRoutes {
                slots: vec![crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-sonnet-1".to_string(),
                    tier: crate::aggregate::AggregateTier::Sonnet,
                    provider_id: "glm-target".to_string(),
                    upstream_model: "glm-5.3".to_string(),
                    label: None,
                    supports_1m: false,
                    max_effort: None,
                }],
                default_target: crate::aggregate::DefaultTarget::ProviderId(
                    "default-target".to_string(),
                ),
                default_model: None,
                alias_rules: vec![],
            }),
            ..Default::default()
        });
        db.save_provider("claude-desktop", &aggregate)
            .expect("save aggregate provider");
        db.set_current_provider("claude-desktop", "agg")
            .expect("select aggregate provider");

        let token = crate::claude_desktop_config::get_or_create_gateway_token(db.as_ref())
            .expect("gateway token");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: true,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();
        let messages_url = format!(
            "http://127.0.0.1:{}/claude-desktop/v1/messages",
            proxy_info.port
        );

        // 命中：model = 槽位 ID → 目标供应商 + 上游模型名改写
        let hit = client
            .post(&messages_url)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&json!({
                "model": "claude-sonnet-1",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .expect("send aggregate hit request");
        assert_eq!(hit.status(), StatusCode::OK, "aggregate hit");

        {
            let captured = captured.lock().await;
            assert_eq!(captured.len(), 1, "hit reached upstream exactly once");
            let request = &captured[0];
            assert_eq!(
                request.body["model"], "glm-5.3",
                "slot upstream model must reach the outbound body"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer target-secret"),
                "auth must come from the slot target, not the aggregate provider"
            );
        }

        // 聚合供应商必须仍是持久化的当前供应商（UI 不得被切走）；同时不得发生伪故障转移。
        assert_eq!(
            db.get_current_provider("claude-desktop")
                .expect("read current provider")
                .as_deref(),
            Some("agg"),
            "aggregate provider must stay the persisted current provider"
        );
        assert_eq!(
            proxy.get_status().await.failover_count,
            0,
            "aggregate routing must not be mistaken for a failover switch"
        );

        // 未命中：不匹配任何槽位 → 走默认目标，聚合层不改写模型名
        let miss = client
            .post(&messages_url)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&json!({
                "model": "claude-haiku-4-5",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .expect("send aggregate miss request");
        assert_eq!(miss.status(), StatusCode::OK, "aggregate miss");

        {
            let captured = captured.lock().await;
            assert_eq!(captured.len(), 2, "miss reached upstream");
            let request = &captured[1];
            assert_eq!(
                request.body["model"], "claude-haiku-4-5",
                "miss must not apply the slot's upstream model"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer default-secret"),
                "miss must be routed to the default target"
            );
        }

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }

    /// 起一个只回固定 JSON 的 mock 上游，返回 (地址, 捕获句柄, 后台任务)。
    /// `response_body` 按路径复用：原生 Responses、Anthropic 转换与 Chat 转换三条
    /// 路径各回自己的形状，与 `handle_responses_for_app` 的分支一一对应。
    async fn spawn_mock_responses_upstream(
        responses_body: &'static str,
        messages_body: &'static str,
    ) -> (
        std::net::SocketAddr,
        Arc<Mutex<Vec<CapturedRequest>>>,
        tokio::task::JoinHandle<()>,
    ) {
        spawn_mock_codex_upstream(responses_body, messages_body, CODEX_CHAT_COMPLETION_BODY).await
    }

    /// `spawn_mock_responses_upstream` 的可换 body 版：Chat 路径要回 Chat 形状的
    /// `chat.completion`，而 Chat→Responses 转换器的入参形状与 Responses 不同，
    /// 不能复用 Responses 那份固定体。
    async fn spawn_mock_codex_upstream(
        responses_body: &'static str,
        messages_body: &'static str,
        chat_body: &'static str,
    ) -> (
        std::net::SocketAddr,
        Arc<Mutex<Vec<CapturedRequest>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let captured = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
        let capture = |captured: Arc<Mutex<Vec<CapturedRequest>>>, response_body: &'static str| {
            move |request: axum::extract::Request| {
                let captured = captured.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let body = axum::body::to_bytes(body, 1024 * 1024)
                        .await
                        .expect("read mock request body");
                    captured.lock().await.push(CapturedRequest {
                        path_and_query: parts
                            .uri
                            .path_and_query()
                            .map(|value| value.as_str().to_string())
                            .unwrap_or_else(|| parts.uri.path().to_string()),
                        authorization: parts
                            .headers
                            .get(header::AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(ToString::to_string),
                        body: serde_json::from_slice(&body).expect("parse mock request body"),
                    });

                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        response_body,
                    )
                }
            }
        };
        let mock_app = Router::new()
            .route(
                "/v1/responses",
                post(capture(captured.clone(), responses_body)),
            )
            .route(
                "/v1/messages",
                post(capture(captured.clone(), messages_body)),
            )
            .route(
                "/v1/chat/completions",
                post(capture(captured.clone(), chat_body)),
            );
        let mock_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind mock upstream");
        let mock_addr = mock_listener.local_addr().expect("mock upstream address");
        let mock_handle = tokio::spawn(async move {
            axum::serve(mock_listener, mock_app)
                .await
                .expect("serve mock upstream");
        });

        (mock_addr, captured, mock_handle)
    }

    /// Codex 聚合供应商（无端点无凭据，只有一张路由表）。
    fn codex_aggregate(
        slots: Vec<crate::aggregate::CodexAggregateSlot>,
        default_target: crate::aggregate::DefaultTarget,
    ) -> Provider {
        let mut provider = Provider::with_id(
            "codex-agg".to_string(),
            "Codex Aggregate".to_string(),
            json!({ "env": {} }),
            None,
        );
        provider.meta = Some(ProviderMeta {
            codex_aggregate_routes: Some(crate::aggregate::CodexAggregateRoutes {
                slots,
                default_target,
                default_model: None,
            }),
            ..Default::default()
        });
        provider
    }

    fn codex_slot(
        model: &str,
        provider_id: &str,
        upstream_model: &str,
    ) -> crate::aggregate::CodexAggregateSlot {
        crate::aggregate::CodexAggregateSlot {
            model: model.to_string(),
            provider_id: provider_id.to_string(),
            upstream_model: upstream_model.to_string(),
            label: None,
        }
    }

    /// 最小可用的 Responses 请求体（与 `handle_responses_for_app` 解析的形状一致）。
    fn codex_responses_request(model: &str) -> Value {
        json!({
            "model": model,
            "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
            "stream": false,
        })
    }

    #[tokio::test]
    async fn codex_aggregate_native_passthrough_rewrites_model_and_credentials() {
        let (mock_addr, captured, mock_handle) = spawn_mock_responses_upstream(
            r#"{"id":"resp_1","object":"response","status":"completed","model":"kimi-k2","output":[],"usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}"#,
            r#"{"id":"msg_1","type":"message","role":"assistant","model":"kimi-k2","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":3,"output_tokens":2}}"#,
        )
        .await;

        let db = Arc::new(Database::memory().expect("memory database"));

        // 槽位目标：端点与凭据都来自它自己
        let target = Provider::with_id(
            "codex-target".to_string(),
            "Codex Target".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "target-secret"}
            }),
            None,
        );
        db.save_provider("codex", &target)
            .expect("save slot target");

        // 默认目标：未命中路由时兜底
        let default_target = Provider::with_id(
            "codex-default".to_string(),
            "Codex Default".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "default-secret"},
                "model": "default-model"
            }),
            None,
        );
        db.save_provider("codex", &default_target)
            .expect("save default target");

        let aggregate = codex_aggregate(
            vec![codex_slot("gpt-5.1", "codex-target", "kimi-k2")],
            crate::aggregate::DefaultTarget::ProviderId("codex-default".to_string()),
        );
        db.save_provider("codex", &aggregate)
            .expect("save codex aggregate");
        db.set_current_provider("codex", "codex-agg")
            .expect("select codex aggregate provider");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: true,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();
        let responses_url = format!("http://127.0.0.1:{}/v1/responses", proxy_info.port);

        // 命中：model = 槽位 model → 目标供应商 + 上游模型名改写（原生透传路径）
        let hit = client
            .post(&responses_url)
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&codex_responses_request("gpt-5.1"))
            .send()
            .await
            .expect("send codex aggregate hit request");
        assert_eq!(
            hit.status(),
            StatusCode::OK,
            "aggregate hit: {}",
            hit.text().await.unwrap_or_default()
        );

        {
            let captured = captured.lock().await;
            assert_eq!(captured.len(), 1, "hit reached upstream exactly once");
            let request = &captured[0];
            assert_eq!(
                request.body["model"], "kimi-k2",
                "slot upstream model must reach the outbound body on native passthrough"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer target-secret"),
                "auth must come from the slot target, not the aggregate provider"
            );
        }

        // 聚合供应商必须仍是持久化的当前供应商（UI 不得被切走）；不得发生伪故障转移。
        assert_eq!(
            db.get_current_provider("codex")
                .expect("read current provider")
                .as_deref(),
            Some("codex-agg"),
            "codex aggregate provider must stay the persisted current provider"
        );
        assert_eq!(
            proxy.get_status().await.failover_count,
            0,
            "codex aggregate routing must not be mistaken for a failover switch"
        );

        // 未命中：不匹配任何槽位 → 走默认目标，聚合层不改写模型名
        let miss = client
            .post(&responses_url)
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&codex_responses_request("gpt-9.9"))
            .send()
            .await
            .expect("send codex aggregate miss request");
        assert_eq!(miss.status(), StatusCode::OK, "aggregate miss");

        {
            let captured = captured.lock().await;
            assert_eq!(captured.len(), 2, "miss reached upstream");
            let request = &captured[1];
            assert_eq!(
                request.body["model"], "gpt-9.9",
                "miss must not apply the slot's upstream model"
            );
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer default-secret"),
                "miss must be routed to the default target"
            );
        }

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }

    #[tokio::test]
    async fn codex_aggregate_skips_apply_codex_upstream_model_when_overridden() {
        let (mock_addr, captured, mock_handle) = spawn_mock_responses_upstream(
            r#"{"id":"resp_1","object":"response","status":"completed","model":"kimi-k2","output":[],"usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}"#,
            r#"{"id":"msg_1","type":"message","role":"assistant","model":"kimi-k2","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":3,"output_tokens":2}}"#,
        )
        .await;

        let db = Arc::new(Database::memory().expect("memory database"));

        // 目标供应商带顶层 model 与 modelCatalog：若聚合改写后又被它们二次改写，
        // 出站模型名就会变成别的模型（聚合层拥有模型名语义，必须原样送达）。
        // `api_format: "anthropic"` 让这条请求走 Responses→Anthropic 转换路径 ——
        // `apply_codex_upstream_model` 就在该路径上被调用。
        let target = Provider::with_id(
            "codex-catalog-target".to_string(),
            "Codex Catalog Target".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "target-secret"},
                "api_format": "anthropic",
                "model": "catalog-default-model",
                "modelCatalog": {"models": [{"model": "some-other-model"}]}
            }),
            None,
        );
        db.save_provider("codex", &target)
            .expect("save catalog target");

        let aggregate = codex_aggregate(
            vec![codex_slot("gpt-5.1", "codex-catalog-target", "kimi-k2")],
            crate::aggregate::DefaultTarget::ProviderId("codex-catalog-target".to_string()),
        );
        db.save_provider("codex", &aggregate)
            .expect("save codex aggregate");
        db.set_current_provider("codex", "codex-agg")
            .expect("select codex aggregate provider");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: true,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();

        let response = client
            .post(format!("http://127.0.0.1:{}/v1/responses", proxy_info.port))
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&codex_responses_request("gpt-5.1"))
            .send()
            .await
            .expect("send codex aggregate request");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "aggregate hit: {}",
            response.text().await.unwrap_or_default()
        );

        {
            let captured = captured.lock().await;
            assert_eq!(captured.len(), 1, "request reached upstream exactly once");
            assert_eq!(
                captured[0].path_and_query.split('?').next().unwrap(),
                "/v1/messages",
                "this case must exercise the Responses→Anthropic conversion path"
            );
            assert_eq!(
                captured[0].body["model"], "kimi-k2",
                "aggregate rewrite must survive the target provider's modelCatalog / default model"
            );
        }

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }

    /// Chat 臂的同款守卫用例。`forwarder.rs` 里 Responses→Chat 与
    /// Responses→Anthropic 两条转换各有**一个独立的** `aggregate_override.is_none()`
    /// 守卫，上一条只钉住了 Anthropic 那一个——删掉 Chat 臂的守卫仍然全绿。而
    /// Chat 是第三方 Codex 网关最常见的上游形状，这条空白恰好盖住最常见的那类目标。
    #[tokio::test]
    async fn codex_aggregate_skips_apply_codex_chat_upstream_model_when_overridden() {
        let (mock_addr, captured, mock_handle) = spawn_mock_codex_upstream(
            r#"{"id":"resp_1","object":"response","status":"completed","model":"kimi-k2","output":[],"usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}"#,
            r#"{"id":"msg_1","type":"message","role":"assistant","model":"kimi-k2","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":3,"output_tokens":2}}"#,
            CODEX_CHAT_COMPLETION_BODY,
        )
        .await;

        let db = Arc::new(Database::memory().expect("memory database"));

        // 与 Anthropic 臂同款构造：`modelCatalog` 只含 `some-other-model`，所以
        // 「kimi-k2」既不在白名单里、也不等于顶层 `model`。守卫一旦缺失，
        // `apply_codex_chat_upstream_model` 就会用 `codex_provider_upstream_model`
        // 把 body.model 改写成 `catalog-default-model` —— 断言因此有判别力。
        // `api_format: "openai_chat"` 让请求走 Responses→Chat 转换路径 ——
        // `apply_codex_chat_upstream_model` 就在该路径上被调用。
        let target = Provider::with_id(
            "codex-chat-target".to_string(),
            "Codex Chat Target".to_string(),
            json!({
                "base_url": format!("http://{mock_addr}/v1"),
                "auth": {"OPENAI_API_KEY": "target-secret"},
                "api_format": "openai_chat",
                "model": "catalog-default-model",
                "modelCatalog": {"models": [{"model": "some-other-model"}]}
            }),
            None,
        );
        db.save_provider("codex", &target)
            .expect("save chat catalog target");

        let aggregate = codex_aggregate(
            vec![codex_slot("gpt-5.1", "codex-chat-target", "kimi-k2")],
            crate::aggregate::DefaultTarget::ProviderId("codex-chat-target".to_string()),
        );
        db.save_provider("codex", &aggregate)
            .expect("save codex aggregate");
        db.set_current_provider("codex", "codex-agg")
            .expect("select codex aggregate provider");

        let proxy = ProxyServer::new(
            ProxyConfig {
                listen_port: 0,
                enable_logging: true,
                non_streaming_timeout: 10,
                ..ProxyConfig::default()
            },
            db.clone(),
            None,
        );
        let proxy_info = proxy.start().await.expect("start test proxy");
        let client = reqwest::Client::new();

        let response = client
            .post(format!("http://127.0.0.1:{}/v1/responses", proxy_info.port))
            .header(header::AUTHORIZATION, "Bearer client-secret")
            .json(&codex_responses_request("gpt-5.1"))
            .send()
            .await
            .expect("send codex aggregate chat request");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "aggregate chat hit: {}",
            response.text().await.unwrap_or_default()
        );

        {
            let captured = captured.lock().await;
            assert_eq!(captured.len(), 1, "request reached upstream exactly once");
            assert!(
                captured[0]
                    .path_and_query
                    .split('?')
                    .next()
                    .unwrap()
                    .ends_with("/chat/completions"),
                "this case must exercise the Responses→Chat conversion path, got {}",
                captured[0].path_and_query
            );
            assert_eq!(
                captured[0].body["model"], "kimi-k2",
                "aggregate rewrite must survive the target provider's modelCatalog / default model \
                 on the Chat arm"
            );
        }

        proxy.stop().await.expect("stop test proxy");
        mock_handle.abort();
    }
}
