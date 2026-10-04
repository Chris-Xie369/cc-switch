//! Account and request-origin regressions. Synthetic accounts and loopback HTTP only.
use super::*;
use crate::aggregate::{CodexAggregateRoutes, CodexAggregateSlot, DefaultTarget};
use crate::database::Database;
use crate::provider::{AuthBinding, AuthBindingSource, ProviderMeta};
use crate::proxy::handler_context::RequestContext;
use crate::proxy::providers::AuthInfo;
use crate::proxy::server::ProxyState;
use crate::proxy::types::ProxyConfig;
use std::time::Duration;

pub(super) struct TestHome {
    _directory: tempfile::TempDir,
    previous: Option<std::ffi::OsString>,
}

impl TestHome {
    pub(super) fn with_unrelated_login() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join(".codex")).unwrap();
        std::fs::write(
            directory.path().join(".codex/auth.json"),
            serde_json::to_vec(&serde_json::json!({
                "auth_mode": "chatgpt",
                "tokens": {"access_token": "SYNTHETIC-UNRELATED-TOKEN", "account_id": "unrelated-workspace"}
            })).unwrap(),
        ).unwrap();
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", directory.path());
        Self {
            _directory: directory,
            previous,
        }
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
    }
}

fn official(account: Option<&str>) -> Provider {
    let mut provider = Provider::with_id(
        crate::database::CODEX_OFFICIAL_PROVIDER_ID.to_string(),
        "Synthetic Official".to_string(),
        serde_json::json!({"auth": {}, "config": ""}),
        None,
    );
    provider.category = Some("official".into());
    provider.meta = Some(ProviderMeta {
        auth_binding: account.map(|id| AuthBinding {
            source: AuthBindingSource::ManagedAccount,
            auth_provider: Some("codex_oauth".into()),
            account_id: Some(id.into()),
        }),
        ..Default::default()
    });
    provider
}

async fn manager(
    token: &str,
    workspace: &str,
    identity: bool,
) -> (tempfile::TempDir, Arc<CodexOAuthManager>) {
    let directory = tempfile::tempdir().unwrap();
    let manager = Arc::new(CodexOAuthManager::new(directory.path().into()));
    let id_token = identity.then(|| crate::codex_config::test_codex_id_token("synthetic-user"));
    manager
        .add_test_account_with_workspace_and_access_token(
            "selected",
            workspace,
            token,
            id_token.as_deref(),
        )
        .await
        .unwrap();
    (directory, manager)
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_removed_binding_rejects_unrelated_live_login() {
    let _home = TestHome::with_unrelated_login();
    let (_dir, manager) = manager("SYNTHETIC-SELECTED-TOKEN", "selected-workspace", true).await;
    let result =
        resolve_aggregate_codex_oauth_credentials_from(&manager, &official(Some("removed"))).await;
    assert!(
        result.is_err(),
        "Removed explicit binding must not authenticate with an unrelated login"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_invalid_default_identity_rejects_unrelated_login() {
    let _home = TestHome::with_unrelated_login();
    let (_dir, manager) = manager("SYNTHETIC-SELECTED-TOKEN", "selected-workspace", false).await;
    let result = resolve_aggregate_codex_oauth_credentials_from(&manager, &official(None)).await;
    assert!(
        result.is_err(),
        "Invalid selected default account must not fall through to another login"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_no_managed_account_does_not_adopt_unselected_login() {
    let _home = TestHome::with_unrelated_login();
    let directory = tempfile::tempdir().unwrap();
    let manager = CodexOAuthManager::new(directory.path().into());
    assert!(
        resolve_aggregate_codex_oauth_credentials_from(&manager, &official(None))
            .await
            .is_err(),
        "An unselected file login is not a managed default account"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_empty_token_or_workspace_is_rejected() {
    let _home = TestHome::with_unrelated_login();
    for (token, workspace) in [("", "selected-workspace"), ("SYNTHETIC-SELECTED-TOKEN", "")] {
        let (_dir, manager) = manager(token, workspace, true).await;
        assert!(
            resolve_aggregate_codex_oauth_credentials_from(&manager, &official(Some("selected")))
                .await
                .is_err(),
            "Empty authentication fields must fail before headers are assembled"
        );
    }
}

struct LoopbackAdapter {
    origin: String,
    inner: Box<dyn ProviderAdapter>,
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_refresh_and_transport_errors_never_change_accounts_or_echo_secrets() {
    use crate::proxy::providers::codex_oauth_auth::CodexOAuthError;
    let _home = TestHome::with_unrelated_login();
    let (_dir, manager) = manager("SYNTHETIC-SELECTED-TOKEN", "selected-workspace", true).await;
    for binding in [Some("selected"), None] {
        for error in [
            CodexOAuthError::RefreshTokenInvalid,
            CodexOAuthError::NetworkError("SYNTHETIC-SECRET-IN-ERROR".into()),
        ] {
            manager.fail_next_token_resolution_for_test(error).await;
            let error =
                resolve_aggregate_codex_oauth_credentials_from(&manager, &official(binding))
                    .await
                    .err()
                    .expect("Token resolution failure must fail closed");
            let message = error.to_string();
            assert!(message.contains("OpenAI 官方"));
            assert!(!message.contains("SYNTHETIC-SECRET"));
        }
    }
}
impl ProviderAdapter for LoopbackAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn extract_base_url(&self, _: &Provider) -> Result<String, ProxyError> {
        Ok(self.origin.clone())
    }
    fn extract_auth(&self, provider: &Provider) -> Option<AuthInfo> {
        self.inner.extract_auth(provider)
    }
    fn build_url(&self, base: &str, endpoint: &str) -> String {
        self.inner.build_url(base, endpoint)
    }
    fn get_auth_headers(
        &self,
        auth: &AuthInfo,
    ) -> Result<Vec<(http::HeaderName, http::HeaderValue)>, ProxyError> {
        self.inner.get_auth_headers(auth)
    }
}

fn proxy_state(db: Arc<Database>) -> ProxyState {
    ProxyState {
        db: db.clone(),
        config: Arc::new(RwLock::new(ProxyConfig::default())),
        status: Arc::new(RwLock::new(ProxyStatus::default())),
        start_time: Arc::new(RwLock::new(None)),
        current_providers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        provider_router: Arc::new(ProviderRouter::new(db.clone())),
        gemini_shadow: Arc::new(GeminiShadowStore::default()),
        codex_chat_history: Arc::new(CodexChatHistoryStore::default()),
        app_handle: None,
        failover_manager: Arc::new(FailoverSwitchManager::new(db)),
    }
}

async fn observed_forward(
    context: &RequestContext,
    state: &ProxyState,
    body: &Value,
    headers: &http::HeaderMap,
    manager: Option<Arc<CodexOAuthManager>>,
) -> (http::HeaderMap, Value) {
    let mut forwarder = context.create_forwarder(&state);
    forwarder.test_codex_oauth_manager = manager;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Arc::new(std::sync::Mutex::new(Some(tx)));
    let app = axum::Router::new().fallback(axum::routing::post(move |headers:http::HeaderMap, axum::Json(body):axum::Json<Value>| {
        let sender = sender.clone();
        async move {
        if let Some(tx)=sender.lock().unwrap().take() {let _=tx.send((headers,body));}
        axum::Json(serde_json::json!({"id":"mock-response","object":"response","status":"completed","output":[]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let adapter = LoopbackAdapter {
        origin,
        inner: crate::proxy::providers::get_adapter(&AppType::Codex).unwrap(),
    };
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let result = forwarder
        .forward(
            &AppType::Codex,
            &http::Method::POST,
            &context.provider,
            "/responses",
            body,
            headers,
            &Extensions::new(),
            &adapter,
        )
        .await;
    server.abort();
    assert!(
        result.is_ok(),
        "Aggregate routing must authenticate independently of model rewrite: {:?}",
        result.err()
    );
    let observed = tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .unwrap()
        .unwrap();
    observed
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_provider_fallback_and_slots_forward_selected_account() {
    let _home = TestHome::with_unrelated_login();
    let (_dir, manager) = manager("SYNTHETIC-SELECTED-TOKEN", "selected-workspace", true).await;
    for (default, request_model, expected_model) in [
        (
            DefaultTarget::ProviderId(crate::database::CODEX_OFFICIAL_PROVIDER_ID.into()),
            "unknown-model",
            "unknown-model",
        ),
        (
            DefaultTarget::SlotId("client-model".into()),
            "unknown-model",
            "upstream-model",
        ),
        (
            DefaultTarget::ProviderId(crate::database::CODEX_OFFICIAL_PROVIDER_ID.into()),
            "client-model",
            "upstream-model",
        ),
    ] {
        let db = Arc::new(Database::memory().unwrap());
        let target = official(Some("selected"));
        db.save_provider("codex", &target).unwrap();
        let mut aggregate = Provider::with_id(
            "synthetic-aggregate".into(),
            "Synthetic Aggregate".into(),
            serde_json::json!({}),
            None,
        );
        aggregate.meta = Some(ProviderMeta {
            codex_aggregate_routes: Some(CodexAggregateRoutes {
                slots: vec![CodexAggregateSlot {
                    model: "client-model".into(),
                    provider_id: target.id.clone(),
                    upstream_model: "upstream-model".into(),
                    label: None,
                }],
                default_target: default,
                default_model: None,
            }),
            ..Default::default()
        });
        db.save_provider("codex", &aggregate).unwrap();
        db.set_current_provider("codex", &aggregate.id).unwrap();
        let state = proxy_state(db);
        let body = serde_json::json!({"model":request_model,"input":[],"stream":false});
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );
        headers.insert(
            "chatgpt-account-id",
            http::HeaderValue::from_static("unrelated-workspace"),
        );
        let context =
            RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex")
                .await
                .unwrap();
        let (sent_headers, sent_body) =
            observed_forward(&context, &state, &body, &headers, Some(manager.clone())).await;
        assert_eq!(
            sent_headers[http::header::AUTHORIZATION],
            "Bearer SYNTHETIC-SELECTED-TOKEN"
        );
        assert_eq!(sent_headers["chatgpt-account-id"], "selected-workspace");
        assert_eq!(sent_body["model"], expected_model);
    }
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_default_account_is_preserved_without_explicit_binding() {
    let _home = TestHome::with_unrelated_login();
    let (_dir, manager) = manager("SYNTHETIC-DEFAULT-TOKEN", "default-workspace", true).await;
    let result = resolve_aggregate_codex_oauth_credentials_from(&manager, &official(None))
        .await
        .unwrap();
    assert_eq!(
        result,
        ("SYNTHETIC-DEFAULT-TOKEN".into(), "default-workspace".into())
    );
}

#[tokio::test]
#[serial_test::serial]
async fn aggregate_auth_normal_official_and_api_key_requests_keep_their_authentication() {
    let _home = TestHome::with_unrelated_login();
    let (_dir, manager) = manager("SYNTHETIC-WRONG-IF-INJECTED", "wrong-if-injected", true).await;
    let mut api_target = Provider::with_id(
        "synthetic-api".into(),
        "Synthetic API".into(),
        serde_json::json!({"base_url":"https://relay.example.invalid/v1","auth":{"OPENAI_API_KEY":"SYNTHETIC-API-KEY"}}),
        None,
    );
    api_target.meta = Some(ProviderMeta::default());
    for (target, expected) in [
        (official(None), "Bearer SYNTHETIC-CLIENT-TOKEN"),
        (api_target, "Bearer SYNTHETIC-API-KEY"),
    ] {
        let db = Arc::new(Database::memory().unwrap());
        db.save_provider("codex", &target).unwrap();
        db.set_current_provider("codex", &target.id).unwrap();
        let state = proxy_state(db);
        let body = serde_json::json!({"model":"unchanged-client-model","input":[],"stream":false});
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer SYNTHETIC-CLIENT-TOKEN"),
        );
        let context =
            RequestContext::new(&state, &body, &headers, AppType::Codex, "Codex", "codex")
                .await
                .unwrap();
        let (sent_headers, sent_body) =
            observed_forward(&context, &state, &body, &headers, Some(manager.clone())).await;
        assert_eq!(sent_headers[http::header::AUTHORIZATION], expected);
        assert_eq!(sent_body["model"], "unchanged-client-model");
    }
}
