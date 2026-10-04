use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
use crate::config::get_home_dir;
use crate::config::{atomic_write, delete_file, read_json_file, write_json_file};
use crate::database::Database;
use crate::database::CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID;
use crate::error::AppError;
use crate::provider::{ClaudeDesktopMode, Provider};
use crate::settings::{get_settings, ClaudeDesktopDisplaySettings};

pub const PROFILE_ID: &str = "00000000-0000-4000-8000-000000157210";
pub const PROFILE_NAME: &str = "CC Switch";

#[cfg(any(target_os = "macos", windows, target_os = "linux", test))]
const CONFIG_FILE: &str = "claude_desktop_config.json";
#[cfg(any(target_os = "macos", windows, target_os = "linux", test))]
const CONFIG_LIBRARY_DIR: &str = "configLibrary";
const GATEWAY_TOKEN_SETTING_KEY: &str = "claude_desktop_gateway_token";
const CLAUDE_DESKTOP_PROXY_PREFIX: &str = "/claude-desktop";
const DEFAULT_CREATED_AT: &str = "2024-01-01T00:00:00Z";
const MIMO_REDACTED_THINKING_PLACEHOLDER: &str = "[redacted thinking]";
const MIMO_TOOL_CALL_THINKING_PLACEHOLDER: &str = "tool call";

/// Claude Desktop 模型菜单识别的 route ID 前缀。
pub const CLAUDE_ROUTE_PREFIX: &str = "claude-";
/// 替代前缀（与前端 `ANTHROPIC_CLAUDE_ROUTE_PREFIX` 一致）。
pub const ANTHROPIC_CLAUDE_ROUTE_PREFIX: &str = "anthropic/claude-";
/// Claude Code env 中通过 `[1M]` 后缀声明 1M 上下文能力（匹配用 `eq_ignore_ascii_case`）。
/// Claude Desktop schema 不接受此后缀，import 边界翻译为 `supports1m` 字段。
pub const ONE_M_CONTEXT_MARKER: &str = "[1m]";

const CURRENT_OPUS_ROUTE_ID: &str = "claude-opus-5";
const LEGACY_OPUS_ROUTE_ID: &str = "claude-opus-4-8";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopDefaultRoute {
    pub route_id: &'static str,
    pub env_key: &'static str,
    #[serde(rename = "supports1m")]
    pub supports_1m: bool,
}

pub const DEFAULT_PROXY_ROUTES: &[ClaudeDesktopDefaultRoute] = &[
    ClaudeDesktopDefaultRoute {
        route_id: "claude-sonnet-5",
        env_key: "ANTHROPIC_DEFAULT_SONNET_MODEL",
        supports_1m: true,
    },
    ClaudeDesktopDefaultRoute {
        route_id: CURRENT_OPUS_ROUTE_ID,
        env_key: "ANTHROPIC_DEFAULT_OPUS_MODEL",
        supports_1m: true,
    },
    ClaudeDesktopDefaultRoute {
        route_id: "claude-haiku-4-5",
        env_key: "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        supports_1m: true,
    },
    // fable 置于末尾：next_catalog_safe_route_id 给非安全品牌 route 借用合法
    // 角色名时仍按 sonnet→opus→haiku 顺序分配（向后兼容既有 catalog），不会把
    // 无关品牌模型借用成 fable 顶配档名。UI 行序由前端 ROLE_ORDER 独立控制为
    // Sonnet/Opus/Fable/Haiku（所有 proxy 路径都经 normalizeProxyRows 重排），
    // 与此处物理顺序无关。
    ClaudeDesktopDefaultRoute {
        route_id: "claude-fable-5",
        env_key: "ANTHROPIC_DEFAULT_FABLE_MODEL",
        supports_1m: true,
    },
];

#[derive(Debug, Clone)]
struct ClaudeDesktopPaths {
    normal_config_path: PathBuf,
    threep_config_path: PathBuf,
    config_library_path: PathBuf,
    profile_path: PathBuf,
    meta_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectGatewayCredentials {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    path: PathBuf,
    content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopStatus {
    pub supported: bool,
    pub configured: bool,
    pub applied_id: Option<String>,
    pub profile_path: Option<String>,
    pub config_library_path: Option<String>,
    pub mode: Option<ClaudeDesktopMode>,
    pub expected_base_url: Option<String>,
    pub actual_base_url: Option<String>,
    pub proxy_running: bool,
    pub stale_raw_models: bool,
    pub missing_route_mappings: bool,
    pub gateway_token_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelRoute {
    pub route_id: String,
    pub upstream_model: String,
    pub label_override: Option<String>,
    pub supports_1m: bool,
    /// 思考强度上限（profile 条目字段 `maxEffort`）。仅聚合槽位会带出，
    /// 普通供应商路径恒为 `None`。
    pub max_effort: Option<String>,
    /// 聚合路由的档位（fable/opus/sonnet/haiku）。`Some` 时按
    /// 「供应商分组 × 档位强弱」排序写 profile，避免不同供应商的模型在
    /// Claude Desktop 的选择器里交错；普通供应商无档位概念，恒为 `None`。
    pub tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InferenceModelSpec {
    name: String,
    label_override: Option<String>,
    supports_1m: bool,
    max_effort: Option<String>,
}

pub fn apply_provider(db: &Database, provider: &Provider) -> Result<(), AppError> {
    let paths = current_platform_paths()?;
    let routes = apply_provider_to_paths(db, provider, &paths)?;
    // picker 是衍生件：profile 已落盘，同步失败只告警不撤回（下次 apply 自愈）
    if let Err(err) = crate::model_picker::sync_cli_model_picker(db, routes.as_deref()) {
        log::warn!("modelPicker 同步失败（下次 apply 自愈）: {err}");
    }
    Ok(())
}

pub fn get_status(db: &Database, proxy_running: bool) -> Result<ClaudeDesktopStatus, AppError> {
    if !is_supported_platform() {
        return Ok(ClaudeDesktopStatus {
            supported: false,
            configured: false,
            applied_id: None,
            profile_path: None,
            config_library_path: None,
            mode: None,
            expected_base_url: None,
            actual_base_url: None,
            proxy_running,
            stale_raw_models: false,
            missing_route_mappings: false,
            gateway_token_configured: false,
        });
    }

    let paths = current_platform_paths()?;
    let applied_id = read_applied_id(&paths.meta_path);
    let configured = paths.profile_path.exists() || meta_has_profile_entry(&paths.meta_path);
    let profile = read_json_or_empty(&paths.profile_path).unwrap_or_else(|_| json!({}));
    let actual_base_url = profile
        .get("inferenceGatewayBaseUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    let stale_raw_models = profile
        .get("inferenceModels")
        .and_then(Value::as_array)
        .map(|models| {
            models.iter().any(|item| {
                item.as_str()
                    .or_else(|| item.get("name").and_then(Value::as_str))
                    .is_some_and(|model| !is_claude_safe_model_id(model))
            })
        })
        .unwrap_or(false);
    let gateway_token_configured = db
        .get_setting(GATEWAY_TOKEN_SETTING_KEY)
        .ok()
        .flatten()
        .is_some_and(|token| !token.trim().is_empty());
    let current_provider = crate::settings::get_effective_current_provider(
        db,
        &crate::app_config::AppType::ClaudeDesktop,
    )
    .ok()
    .flatten()
    .and_then(|id| db.get_provider_by_id(&id, "claude-desktop").ok().flatten());
    let mode = current_provider.as_ref().map(provider_mode);
    let expected_base_url = match mode {
        Some(ClaudeDesktopMode::Proxy) => proxy_gateway_base_url_from_db(db).ok(),
        Some(ClaudeDesktopMode::Direct) => current_provider
            .as_ref()
            .and_then(|provider| direct_gateway_credentials(provider).ok())
            .map(|credentials| credentials.base_url),
        None => None,
    };
    let missing_route_mappings = current_provider.as_ref().is_some_and(|provider| {
        matches!(provider_mode(provider), ClaudeDesktopMode::Proxy)
            && proxy_model_routes(provider).is_err()
    });

    Ok(ClaudeDesktopStatus {
        supported: true,
        configured,
        applied_id,
        profile_path: Some(paths.profile_path.display().to_string()),
        config_library_path: Some(paths.config_library_path.display().to_string()),
        mode,
        expected_base_url,
        actual_base_url,
        proxy_running,
        stale_raw_models,
        missing_route_mappings,
        gateway_token_configured,
    })
}

pub fn get_config_library_path() -> Result<PathBuf, AppError> {
    Ok(current_platform_paths()?.config_library_path)
}

pub fn default_proxy_routes() -> Vec<ClaudeDesktopDefaultRoute> {
    DEFAULT_PROXY_ROUTES.to_vec()
}

pub fn is_compatible_direct_provider(provider: &Provider) -> bool {
    validate_direct_provider(provider).is_ok()
}

pub fn is_official_provider(provider: &Provider) -> bool {
    provider.id == CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID
}

pub fn provider_mode(provider: &Provider) -> ClaudeDesktopMode {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.claude_desktop_mode.clone())
        .unwrap_or(ClaudeDesktopMode::Direct)
}

pub fn is_claude_safe_model_id(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.contains(ONE_M_CONTEXT_MARKER) {
        return false;
    }
    if has_non_anthropic_vendor_token(&normalized) {
        return false;
    }

    let Some(route_tail) = normalized
        .strip_prefix(ANTHROPIC_CLAUDE_ROUTE_PREFIX)
        .or_else(|| normalized.strip_prefix(CLAUDE_ROUTE_PREFIX))
    else {
        return false;
    };

    // 角色前缀后必须还有实际模型标识，拒绝 claude-sonnet- 这类退化值
    // （否则会写入 profile 并触发 Claude Desktop fail-all 拒收整组）。
    // Claude Desktop 1.12603.1+ 的 fail-all validator 角色白名单已纳入 fable
    // （app.asar 内 ["sonnet","opus","haiku","fable","mythos"]），故 claude-fable-*
    // 可安全写入 profile。mythos 官方未公开发布，暂不暴露给用户。
    ["sonnet-", "opus-", "haiku-", "fable-"]
        .iter()
        .any(|prefix| {
            route_tail
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        })
}

/// Claude Desktop 的**厂商词黑名单**（`app.asar` 内 `Vxe` 的逐字转写）。
///
/// 它的判定是「厂商词优先否决」：`Go(name) = Vxe.test(name) ? false : …`，
/// 即名字里**只要出现** deepseek / glm / kimi / gpt… 任一词，即被判为
/// "is not an Anthropic model" 并从 `inferenceModels` 移除，**且整组生效**——
/// 一个坏 ID 就让所有模型消失、选择器变空。
///
/// 2026-09-22 实测事故：槽位 ID 曾把供应商名 slug 进去（`claude-fable-deepseek`、
/// `claude-fable-zhipu-glm`），恰好全部命中黑名单，四个槽位被 Claude Desktop
/// 悄悄删光。这一层校验当时在 cc-switch 侧是缺的，故补上——前端已改为
/// 「ID 只含档位与序号」，此处是可写入 profile 的最终闸门。
///
/// 大小写不敏感由调用方先 `to_ascii_lowercase` 保证（与 `Vxe` 前的
/// `name.toLowerCase()` 一致），此处不再加 `(?i)`。
fn has_non_anthropic_vendor_token(model: &str) -> bool {
    const VENDOR_TOKENS: &str = r"ark-code|astron|command-r|deepseek|doubao|gemini|gemma|glm|gpt|grok|hermes|hy3|kimi|lfm|\bling\b|llama|longcat|mimo|minimax|mistral|mixtral|moonshot|nemotron|openai|phi-|qianfan|qwen|tc-code|\bunic\b|yi-|stepfun|step-3|seed-|bytedance|hunyuan|granite|amazon\.nova|nova-|devstral|ministral|ernie|codex|arcee|trinity|abab|phi\d|\bk2\.|\bm2\.|jamba|arctic|solar|mercury|zamba|kat-coder|\bds-|dpsk";
    static VENDOR_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    VENDOR_RE
        .get_or_init(|| {
            regex::Regex::new(VENDOR_TOKENS).expect("vendor token pattern is a valid regex")
        })
        .is_match(model)
}

fn inference_model_json(spec: &InferenceModelSpec) -> Value {
    if spec.supports_1m || spec.label_override.is_some() || spec.max_effort.is_some() {
        let mut item = json!({ "name": spec.name });
        if let Some(label_override) = spec.label_override.as_deref() {
            item["labelOverride"] = json!(label_override);
        }
        if spec.supports_1m {
            item["supports1m"] = json!(true);
        }
        if let Some(effort) = spec.max_effort.as_deref() {
            item["maxEffort"] = json!(effort);
        }
        item
    } else {
        Value::String(spec.name.clone())
    }
}

pub fn get_or_create_gateway_token(db: &Database) -> Result<String, AppError> {
    if let Some(token) = db.get_setting(GATEWAY_TOKEN_SETTING_KEY)? {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let token = format!("ccs-{}", uuid::Uuid::new_v4().simple());
    db.set_setting(GATEWAY_TOKEN_SETTING_KEY, &token)?;
    Ok(token)
}

pub fn direct_gateway_credentials(
    provider: &Provider,
) -> Result<DirectGatewayCredentials, AppError> {
    let env = provider
        .settings_config
        .get("env")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.env_missing",
                "Claude Desktop 直连供应商缺少 env 配置",
                "Claude Desktop direct provider is missing env configuration",
            )
        })?;

    let base_url = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.base_url_missing",
                "Claude Desktop 直连供应商缺少 ANTHROPIC_BASE_URL",
                "Claude Desktop direct provider is missing ANTHROPIC_BASE_URL",
            )
        })?
        .to_string();

    let api_key = env
        .get("ANTHROPIC_AUTH_TOKEN")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.auth_token_missing",
                "Claude Desktop 直连供应商缺少 ANTHROPIC_AUTH_TOKEN（Bearer Token）",
                "Claude Desktop direct provider is missing ANTHROPIC_AUTH_TOKEN (Bearer Token)",
            )
        })?
        .to_string();

    Ok(DirectGatewayCredentials { base_url, api_key })
}

pub fn validate_direct_provider(provider: &Provider) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return Ok(());
    }

    // 聚合供应商自身无端点无凭据（转发时用目标供应商的），故不适用直连凭据校验。
    if crate::aggregate::is_aggregate_provider(provider) {
        return Ok(());
    }

    if !provider.settings_config.is_object() {
        return Err(AppError::localized(
            "claude_desktop.provider.settings_not_object",
            "Claude Desktop 直连供应商配置必须是 JSON 对象",
            "Claude Desktop direct provider configuration must be a JSON object",
        ));
    }

    if let Some(meta) = provider.meta.as_ref() {
        if let Some(api_format) = meta.api_format.as_deref() {
            if !api_format.trim().is_empty() && api_format != "anthropic" {
                return Err(AppError::localized(
                    "claude_desktop.provider.api_format_unsupported",
                    "Claude Desktop 第一阶段只支持原生 Anthropic Messages API",
                    "Claude Desktop phase 1 only supports native Anthropic Messages API",
                ));
            }
        }

        if matches!(
            meta.claude_desktop_mode.as_ref(),
            Some(ClaudeDesktopMode::Proxy)
        ) {
            return Err(AppError::localized(
                "claude_desktop.provider.mode_unsupported",
                "该供应商是 Claude Desktop 本地路由模式，不能按直连模式写入",
                "This Claude Desktop provider uses proxy mode and cannot be written as direct mode",
            ));
        }

        if matches!(
            meta.provider_type.as_deref(),
            Some("github_copilot") | Some("codex_oauth") | Some("xai_oauth")
        ) {
            return Err(AppError::localized(
                "claude_desktop.provider.type_unsupported",
                "Claude Desktop 直连模式不支持需要本地代理转换的供应商",
                "Claude Desktop direct mode does not support providers that require local proxy conversion",
            ));
        }

        if meta.is_full_url == Some(true) {
            return Err(AppError::localized(
                "claude_desktop.provider.full_url_unsupported",
                "Claude Desktop 直连模式不支持完整 URL 端点配置",
                "Claude Desktop direct mode does not support full URL endpoint configuration",
            ));
        }
    }

    direct_inference_model_specs(provider)?;
    direct_gateway_credentials(provider)?;
    Ok(())
}

pub fn validate_proxy_provider(provider: &Provider) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return Ok(());
    }

    if !provider.settings_config.is_object() {
        return Err(AppError::localized(
            "claude_desktop.provider.settings_not_object",
            "Claude Desktop 本地路由供应商配置必须是 JSON 对象",
            "Claude Desktop proxy provider configuration must be a JSON object",
        ));
    }

    if let Some(meta) = provider.meta.as_ref() {
        if let Some(api_format) = meta.api_format.as_deref() {
            if !matches!(
                api_format,
                "" | "anthropic" | "openai_chat" | "openai_responses" | "gemini_native"
            ) {
                return Err(AppError::localized(
                    "claude_desktop.provider.api_format_unsupported",
                    format!("Claude Desktop 本地路由模式不支持 API 格式: {api_format}"),
                    format!("Claude Desktop proxy mode does not support API format: {api_format}"),
                ));
            }
        }
    }

    proxy_model_routes(provider)?;

    // 聚合供应商自身无端点无凭据（转发时用目标供应商的），故不检查它自己的凭据；
    // 路由表本身已由上面的 proxy_model_routes 校验。
    if crate::aggregate::is_aggregate_provider(provider) {
        return Ok(());
    }

    if !has_proxy_base_url_and_key(provider) {
        return Err(AppError::localized(
            "claude_desktop.provider.credentials_missing",
            "Claude Desktop 本地路由供应商缺少 Base URL 或 API Key",
            "Claude Desktop proxy provider is missing Base URL or API key",
        ));
    }

    Ok(())
}

fn has_proxy_base_url_and_key(provider: &Provider) -> bool {
    let env = provider.settings_config.get("env");
    let has_base_url = env
        .and_then(|value| value.get("ANTHROPIC_BASE_URL"))
        .or_else(|| provider.settings_config.get("base_url"))
        .or_else(|| provider.settings_config.get("baseURL"))
        .or_else(|| provider.settings_config.get("apiEndpoint"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if is_managed_oauth_proxy_provider(provider) {
        return has_base_url;
    }

    let has_key = env
        .and_then(|value| {
            [
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_API_KEY",
                "OPENROUTER_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
            ]
            .into_iter()
            .find_map(|key| value.get(key))
        })
        .or_else(|| provider.settings_config.get("apiKey"))
        .or_else(|| provider.settings_config.get("api_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    has_base_url && has_key
}

fn is_managed_oauth_proxy_provider(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        .is_some_and(|provider_type| {
            matches!(
                provider_type,
                "github_copilot" | "codex_oauth" | "xai_oauth"
            )
        })
}

pub fn validate_provider(provider: &Provider) -> Result<(), AppError> {
    if is_official_provider(provider) {
        return Ok(());
    }

    match provider_mode(provider) {
        ClaudeDesktopMode::Direct => validate_direct_provider(provider),
        ClaudeDesktopMode::Proxy => validate_proxy_provider(provider),
    }
}

fn direct_inference_model_specs(provider: &Provider) -> Result<Vec<InferenceModelSpec>, AppError> {
    let Some(routes) = provider
        .meta
        .as_ref()
        .map(|meta| &meta.claude_desktop_model_routes)
    else {
        return Ok(Vec::new());
    };

    let mut result = Vec::new();
    for (route_id, route) in routes {
        let supports_1m = route.supports_1m.unwrap_or(false);
        let route_id = route_id.trim();
        if route_id.is_empty() {
            continue;
        }
        if !is_claude_safe_model_id(route_id) {
            return Err(AppError::localized(
                "claude_desktop.provider.route_invalid",
                format!(
                    "Claude Desktop 直连模型必须使用 claude-* 或 anthropic/claude-* 名称: {route_id}"
                ),
                format!(
                    "Claude Desktop direct model must use a claude-* or anthropic/claude-* name: {route_id}"
                ),
            ));
        }
        let upstream_model = route.model.trim();
        if !upstream_model.is_empty() && upstream_model != route_id {
            return Err(AppError::localized(
                "claude_desktop.provider.direct_mapping_unsupported",
                format!(
                    "Claude Desktop 直连模式不能映射模型: {route_id} -> {upstream_model}；非 Claude 官方模型请使用本地路由模式"
                ),
                format!(
                    "Claude Desktop direct mode cannot map models: {route_id} -> {upstream_model}; use proxy mode for non-Claude official models"
                ),
            ));
        }
        result.push(InferenceModelSpec {
            name: route_id.to_string(),
            label_override: route
                .label_override
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            supports_1m,
            max_effort: None,
        });
    }

    // Sort supports_1m=true first within each name so the subsequent dedup_by
    // (which keeps the first occurrence) preserves the 1M-capable variant.
    result.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| b.supports_1m.cmp(&a.supports_1m))
    });
    result.dedup_by(|a, b| a.name == b.name);
    Ok(result)
}

pub fn proxy_model_routes(provider: &Provider) -> Result<Vec<ResolvedModelRoute>, AppError> {
    // 聚合供应商：模型规格由路由表的槽位派生，而不是自身的 claudeDesktopModelRoutes
    if crate::aggregate::is_aggregate_provider(provider) {
        return crate::aggregate::aggregate_model_routes(provider);
    }

    let routes = provider
        .meta
        .as_ref()
        .map(|meta| &meta.claude_desktop_model_routes)
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.routes_missing",
                "Claude Desktop 本地路由模式缺少模型路由映射",
                "Claude Desktop proxy mode is missing model route mappings",
            )
        })?;

    let reserved_route_ids = routes
        .keys()
        .map(|route_id| route_id.trim())
        .filter(|route_id| is_claude_safe_model_id(route_id))
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();
    let mut result = Vec::new();
    let mut entries = routes.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(left, _)| *left);
    for (route_id, route) in entries {
        let supports_1m = route.supports_1m.unwrap_or(false);
        let route_id = route_id.trim();
        let upstream_model = route.model.trim();
        if route_id.is_empty() || upstream_model.is_empty() {
            continue;
        }
        let repaired_route_id = if is_claude_safe_model_id(route_id) {
            route_id.to_string()
        } else {
            next_catalog_safe_route_id(&result, &reserved_route_ids)
        };
        result.push(ResolvedModelRoute {
            route_id: repaired_route_id,
            upstream_model: upstream_model.to_string(),
            label_override: route
                .label_override
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    (!is_claude_safe_model_id(route_id)).then(|| upstream_model.to_string())
                }),
            supports_1m,
            max_effort: None,
            tier: None,
        });
    }

    result.sort_by(|a, b| a.route_id.cmp(&b.route_id));
    result.dedup_by(|a, b| a.route_id == b.route_id);

    if result.is_empty() {
        return Err(AppError::localized(
            "claude_desktop.provider.routes_missing",
            "Claude Desktop 本地路由模式至少需要一个模型路由映射",
            "Claude Desktop proxy mode requires at least one model route mapping",
        ));
    }

    Ok(result)
}

fn next_catalog_safe_route_id(
    existing: &[ResolvedModelRoute],
    reserved: &std::collections::HashSet<String>,
) -> String {
    if let Some(default_route) = DEFAULT_PROXY_ROUTES
        .iter()
        .map(|route| route.route_id)
        .find(|route_id| {
            !reserved.contains(*route_id)
                && !existing.iter().any(|route| route.route_id == *route_id)
        })
    {
        return default_route.to_string();
    }

    let mut index = 2usize;
    loop {
        let route_id = format!("{}-r{index}", DEFAULT_PROXY_ROUTES[0].route_id);
        if !reserved.contains(&route_id) && !existing.iter().any(|route| route.route_id == route_id)
        {
            return route_id;
        }
        index += 1;
    }
}

pub fn model_list_response(provider: &Provider) -> Result<Value, AppError> {
    let routes = proxy_model_routes(provider)?;
    let data: Vec<Value> = routes
        .iter()
        .map(|route| {
            let model_id = route.route_id.clone();
            let mut item = json!({
                "type": "model",
                "id": model_id,
                "created_at": DEFAULT_CREATED_AT,
            });
            if route.supports_1m {
                item["supports1m"] = json!(true);
            }
            item
        })
        .collect();
    let first_id = data
        .first()
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let last_id = data
        .last()
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(json!({
        "data": data,
        "has_more": false,
        "first_id": first_id,
        "last_id": last_id,
    }))
}

pub fn map_proxy_request_model(mut body: Value, provider: &Provider) -> Result<Value, AppError> {
    let requested_raw = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.model_missing",
                "Claude Desktop 请求缺少 model 字段",
                "Claude Desktop request is missing the model field",
            )
        })?;
    let requested = strip_one_m_suffix_for_route_lookup(&requested_raw);

    let routes = proxy_model_routes(provider)?;
    let upstream_model = routes
        .iter()
        .find(|r| r.route_id == requested)
        .or_else(|| {
            routes
                .iter()
                .find(|r| is_compatible_opus_route_alias(&r.route_id, requested))
        })
        .map(|route| route.upstream_model.clone())
        .or_else(|| legacy_raw_route_upstream_model(provider, requested))
        .or_else(|| {
            // 角色关键词回落:Claude Desktop 的部分调用(如子 agent)会请求带发布
            // 日期后缀的完整官方名(claude-haiku-4-5-20251001),与 manifest 暴露的
            // 简短 route_id(claude-haiku-4-5)不精确相等。按 opus/haiku/fable/sonnet
            // 归类到同档已配置路由,对齐 Claude Code model_mapper 的宽松匹配。
            // 匹配前已剥离本地 [1m] 标记；这里仍只对 Claude Desktop 认可的
            // 安全模型名回落，避免非 Claude route 被误映射。
            if !is_claude_safe_model_id(requested) {
                return None;
            }
            let role = claude_role_keyword(requested)?;
            routes
                .iter()
                .find(|route| claude_role_keyword(&route.route_id) == Some(role))
                // 老用户只配了 Sonnet/Opus/Haiku 三档时，fable 请求降级到 opus 档，
                // 与官方安全分类器的降级方向一致，避免 route_unknown 硬错误。
                // 用户一旦显式配置 fable 档，上面的精确角色匹配会优先命中。
                .or_else(|| {
                    (role == "fable")
                        .then(|| {
                            routes
                                .iter()
                                .find(|route| claude_role_keyword(&route.route_id) == Some("opus"))
                        })
                        .flatten()
                })
                .map(|route| route.upstream_model.clone())
        })
        .ok_or_else(|| {
            AppError::localized(
                "claude_desktop.provider.route_unknown",
                format!("Claude Desktop 模型路由未配置: {requested_raw}"),
                format!("Claude Desktop model route is not configured: {requested_raw}"),
            )
        })?;

    body["model"] = json!(upstream_model);
    if should_normalize_mimo_anthropic_thinking_history(provider, &upstream_model) {
        normalize_mimo_anthropic_thinking_history(&mut body);
    }
    Ok(body)
}

/// 剥离模型名尾部的 `[1m]` 标记（大小写不敏感，容忍分隔空白），供路由查找使用。
/// 聚合路由的槽位查找也复用它（见 `aggregate::resolve_target`）。
pub(crate) fn strip_one_m_suffix_for_route_lookup(model: &str) -> &str {
    let trimmed = model.trim();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    trimmed
}

fn legacy_raw_route_upstream_model(provider: &Provider, requested: &str) -> Option<String> {
    provider
        .meta
        .as_ref()?
        .claude_desktop_model_routes
        .iter()
        .find(|(route_id, _)| route_id.trim() == requested)
        .and_then(|(_, route)| {
            let upstream_model = route.model.trim();
            (!upstream_model.is_empty()).then(|| upstream_model.to_string())
        })
}

fn is_compatible_opus_route_alias(route_id: &str, requested: &str) -> bool {
    matches!(
        (route_id, requested),
        (CURRENT_OPUS_ROUTE_ID, LEGACY_OPUS_ROUTE_ID)
            | (LEGACY_OPUS_ROUTE_ID, CURRENT_OPUS_ROUTE_ID)
    )
}

/// 按角色关键词(opus / haiku / fable / sonnet)归类一个 Claude 模型名/route_id。
/// 仅在命中明确角色词时返回 Some,未知模型返回 None(不回落,保持精确报错语义)。
/// 与前端 `routeRoleFromId` 同序(opus → haiku → fable → sonnet)。
fn claude_role_keyword(model: &str) -> Option<&'static str> {
    let normalized = model.to_ascii_lowercase();
    if normalized.contains("opus") {
        Some("opus")
    } else if normalized.contains("haiku") {
        Some("haiku")
    } else if normalized.contains("fable") {
        Some("fable")
    } else if normalized.contains("sonnet") {
        Some("sonnet")
    } else {
        None
    }
}

fn should_normalize_mimo_anthropic_thinking_history(
    provider: &Provider,
    upstream_model: &str,
) -> bool {
    if !provider_uses_anthropic_messages_format(provider) {
        return false;
    }

    is_mimo_identifier(upstream_model) || provider_has_mimo_endpoint(provider)
}

fn provider_uses_anthropic_messages_format(provider: &Provider) -> bool {
    let api_format = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.api_format.as_deref())
        .or_else(|| {
            provider
                .settings_config
                .get("api_format")
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .unwrap_or("anthropic");

    api_format.is_empty() || api_format == "anthropic"
}

fn provider_has_mimo_endpoint(provider: &Provider) -> bool {
    let settings = &provider.settings_config;
    [
        settings
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
            .and_then(Value::as_str),
        settings.get("base_url").and_then(Value::as_str),
        settings.get("baseURL").and_then(Value::as_str),
        settings.get("apiEndpoint").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .any(is_mimo_identifier)
}

fn is_mimo_identifier(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("mimo") || value.contains("xiaomimimo")
}

fn normalize_mimo_anthropic_thinking_history(body: &mut Value) {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };

    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }

        let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        if !content
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        {
            continue;
        }

        let mut has_thinking = false;
        for block in content.iter_mut() {
            match block.get("type").and_then(Value::as_str) {
                Some("thinking") => {
                    let has_non_empty_thinking = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty());
                    if let Some(obj) = block.as_object_mut() {
                        obj.remove("signature");
                    }
                    if has_non_empty_thinking {
                        has_thinking = true;
                    } else if let Some(obj) = block.as_object_mut() {
                        obj.insert(
                            "thinking".to_string(),
                            json!(MIMO_TOOL_CALL_THINKING_PLACEHOLDER),
                        );
                        has_thinking = true;
                    }
                }
                Some("redacted_thinking") => {
                    *block = json!({
                        "type": "thinking",
                        "thinking": MIMO_REDACTED_THINKING_PLACEHOLDER
                    });
                    has_thinking = true;
                }
                _ => {}
            }
        }

        if !has_thinking {
            content.insert(
                0,
                json!({
                    "type": "thinking",
                    "thinking": MIMO_TOOL_CALL_THINKING_PLACEHOLDER
                }),
            );
        }
    }
}

pub fn proxy_gateway_base_url_from_db(db: &Database) -> Result<String, AppError> {
    // get_proxy_config is async-tagged but its body is fully synchronous (rusqlite
    // under a Mutex), so block_on cannot deadlock the calling thread.
    let config = futures::executor::block_on(db.get_proxy_config())?;
    if config.listen_port == 0 {
        return Err(AppError::Config(
            "Claude Desktop 代理地址需要真实监听端口；请先启动本地代理或使用固定端口".to_string(),
        ));
    }
    Ok(format!(
        "{}{}",
        proxy_origin_from_parts(&config.listen_address, config.listen_port),
        CLAUDE_DESKTOP_PROXY_PREFIX
    ))
}

/// 官方恢复与 provider 应用同寿命：picker 也必须被清掉。
fn apply_provider_to_paths(
    db: &Database,
    provider: &Provider,
    paths: &ClaudeDesktopPaths,
) -> Result<Option<Vec<ResolvedModelRoute>>, AppError> {
    if is_official_provider(provider) {
        restore_official_at_paths(paths)?;
        return Ok(None);
    }

    validate_provider(provider)?;
    with_rollback(paths, |paths| {
        apply_provider_to_paths_inner(db, provider, paths)
    })
}

fn restore_official_at_paths(paths: &ClaudeDesktopPaths) -> Result<(), AppError> {
    with_rollback(paths, restore_official_at_paths_inner)
}

fn with_rollback<T, F>(paths: &ClaudeDesktopPaths, op: F) -> Result<T, AppError>
where
    F: FnOnce(&ClaudeDesktopPaths) -> Result<T, AppError>,
{
    let snapshots = snapshot_files(paths)?;
    match op(paths) {
        Ok(value) => Ok(value),
        Err(err) => match restore_snapshots(&snapshots) {
            Ok(()) => Err(err),
            Err(rollback_err) => {
                log::error!("Failed to rollback Claude Desktop config after error: {rollback_err}");
                Err(AppError::Message(format!(
                    "{err}; rollback failed: {rollback_err}"
                )))
            }
        },
    }
}

/// 返回值只给 CLI 的 `modelPicker` 派生用：聚合供应商才带路由，
/// 直连/官方/普通代理（模型映射行）一律 `None`（spec 范围只覆盖聚合）。
fn apply_provider_to_paths_inner(
    db: &Database,
    provider: &Provider,
    paths: &ClaudeDesktopPaths,
) -> Result<Option<Vec<ResolvedModelRoute>>, AppError> {
    // 聚合供应商自身无端点无凭据：它的槽位模型列表与请求转发都依赖本地代理，
    // 故一律按代理分支写 profile（否则会去取聚合自己的端点/凭据而失败——表单默认「直连」）。
    let profile_mode = if crate::aggregate::is_aggregate_provider(provider) {
        ClaudeDesktopMode::Proxy
    } else {
        provider_mode(provider)
    };

    let mut cli_routes = None;
    let mut profile = match profile_mode {
        ClaudeDesktopMode::Direct => {
            let credentials = direct_gateway_credentials(provider)?;
            let model_specs = direct_inference_model_specs(provider)?;
            build_gateway_profile(
                &credentials.base_url,
                &credentials.api_key,
                (!model_specs.is_empty()).then_some(model_specs.as_slice()),
            )
        }
        ClaudeDesktopMode::Proxy => {
            let base_url = proxy_gateway_base_url_from_db(db)?;
            let api_key = get_or_create_gateway_token(db)?;
            let routes = proxy_model_routes(provider)?;
            let model_specs = routes
                .iter()
                .map(|route| InferenceModelSpec {
                    name: route.route_id.clone(),
                    label_override: route.label_override.clone(),
                    supports_1m: route.supports_1m,
                    max_effort: route.max_effort.clone(),
                })
                .collect::<Vec<_>>();
            if crate::aggregate::is_aggregate_provider(provider) {
                cli_routes = Some(routes);
            }
            build_gateway_profile(&base_url, &api_key, Some(model_specs.as_slice()))
        }
    };

    inject_display_settings(&mut profile, get_settings().claude_desktop_display.as_ref());

    write_deployment_mode(&paths.normal_config_path, "3p")?;
    write_deployment_mode(&paths.threep_config_path, "3p")?;
    // Merge with the existing profile instead of overwriting it wholesale, so
    // non-gateway fields Claude Desktop / the user set (managedMcpServers,
    // chatTabEnabled, autoModeEnabled, ...) survive a provider switch.
    let existing = read_json_or_empty(&paths.profile_path)?;
    let merged = merge_profile(&existing, &profile);
    write_json_file(&paths.profile_path, &merged)?;
    write_meta(&paths.meta_path, Some(PROFILE_ID))?;

    Ok(cli_routes)
}

fn restore_official_at_paths_inner(paths: &ClaudeDesktopPaths) -> Result<(), AppError> {
    write_deployment_mode(&paths.normal_config_path, "1p")?;
    write_deployment_mode(&paths.threep_config_path, "1p")?;
    remove_cc_switch_enterprise_config(&paths.threep_config_path)?;

    if paths.profile_path.exists() {
        delete_file(&paths.profile_path)?;
    }
    write_meta(&paths.meta_path, None)?;

    Ok(())
}

/// 把 Claude Desktop 左下角显示设置注入 profile（配置了才写）。
/// 未配置时不动 profile，三键由 merge 语义保留磁盘旧值。
/// 空 name 视为未配置（前端开关以空串做默认种子），同样不写三键。
/// 空 subtitle 也视为未设置：不写该键，磁盘旧副标题由 merge 语义保留，
/// 避免用户启用显示名却只填名字时把已有副标题抹空。
fn inject_display_settings(profile: &mut Value, display: Option<&ClaudeDesktopDisplaySettings>) {
    if let Some(display) = display {
        if display.name.is_empty() {
            return;
        }
        profile["deploymentDisplayName"] = Value::String(display.name.clone());
        if !display.subtitle.is_empty() {
            profile["deploymentDisplaySubtitle"] = Value::String(display.subtitle.clone());
        }
        profile["endUserAttribution"] = Value::Bool(display.attribution);
    }
}

fn build_gateway_profile(
    base_url: &str,
    api_key: &str,
    model_specs: Option<&[InferenceModelSpec]>,
) -> Value {
    let mut profile = json!({
        "coworkEgressAllowedHosts": ["*"],
        "disableDeploymentModeChooser": true,
        "inferenceGatewayApiKey": api_key,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": base_url,
        "inferenceProvider": "gateway"
    });

    if let Some(model_specs) = model_specs {
        profile["inferenceModels"] =
            Value::Array(model_specs.iter().map(inference_model_json).collect());
    }

    profile
}

/// Merge a freshly-built gateway profile with whatever is already on disk.
///
/// `build_gateway_profile` rebuilds only the gateway-related fields on every
/// switch. Writing that object directly would discard non-gateway fields that
/// Claude Desktop or the user relies on -- `managedMcpServers`,
/// `chatTabEnabled`, `autoModeEnabled`, `inferenceCredentialKind`, and the
/// like -- which can trigger a Claude Desktop UI/session reset that wipes
/// plugin marketplaces, skills, and MCP state (issue #3329).
///
/// Strategy: start from the existing profile, overwrite every key the new
/// profile carries (so gateway fields are always refreshed), and preserve any
/// other key already on disk. `inferenceModels` is gateway-owned but
/// conditionally written by `build_gateway_profile`; when the new profile
/// omits it we drop any stale value so the previous provider's model mappings
/// do not leak across a switch.
fn merge_profile(existing: &Value, new_profile: &Value) -> Value {
    let mut merged = existing.clone();
    let Some(merged_obj) = merged.as_object_mut() else {
        return new_profile.clone();
    };
    let Some(new_obj) = new_profile.as_object() else {
        return new_profile.clone();
    };
    for (key, value) in new_obj {
        merged_obj.insert(key.clone(), value.clone());
    }
    if !new_obj.contains_key("inferenceModels") {
        merged_obj.remove("inferenceModels");
    }
    merged
}

fn read_json_or_empty(path: &Path) -> Result<Value, AppError> {
    let value = if path.exists() {
        read_json_file(path)?
    } else {
        json!({})
    };

    if value.is_object() {
        Ok(value)
    } else {
        Ok(json!({}))
    }
}

fn snapshot_files(paths: &ClaudeDesktopPaths) -> Result<Vec<FileSnapshot>, AppError> {
    [
        &paths.normal_config_path,
        &paths.threep_config_path,
        &paths.profile_path,
        &paths.meta_path,
    ]
    .into_iter()
    .map(|path| {
        let content = if path.exists() {
            Some(fs::read(path).map_err(|e| AppError::io(path, e))?)
        } else {
            None
        };
        Ok(FileSnapshot {
            path: path.clone(),
            content,
        })
    })
    .collect()
}

fn restore_snapshots(snapshots: &[FileSnapshot]) -> Result<(), AppError> {
    for snapshot in snapshots {
        match &snapshot.content {
            Some(content) => {
                if let Some(parent) = snapshot.path.parent() {
                    fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
                }
                atomic_write(&snapshot.path, content)?;
            }
            None => {
                delete_file(&snapshot.path)?;
            }
        }
    }
    Ok(())
}

fn write_deployment_mode(path: &Path, mode: &str) -> Result<(), AppError> {
    let mut value = read_json_or_empty(path)?;
    if !value.is_object() {
        value = json!({});
    }
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "deploymentMode".to_string(),
            Value::String(mode.to_string()),
        );
    }
    write_json_file(path, &value)
}

fn remove_cc_switch_enterprise_config(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    let mut value = read_json_or_empty(path)?;
    let Some(obj) = value.as_object_mut() else {
        return Ok(());
    };
    let Some(enterprise) = obj
        .get_mut("enterpriseConfig")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };

    for key in [
        "disableDeploymentModeChooser",
        "inferenceGatewayApiKey",
        "inferenceGatewayAuthScheme",
        "inferenceGatewayBaseUrl",
        "inferenceProvider",
    ] {
        enterprise.remove(key);
    }

    if enterprise.is_empty() {
        obj.remove("enterpriseConfig");
    }

    write_json_file(path, &value)
}

fn write_meta(path: &Path, applied_profile_id: Option<&str>) -> Result<(), AppError> {
    let mut value = read_json_or_empty(path)?;
    if !value.is_object() {
        value = json!({});
    }

    let obj = value.as_object_mut().expect("just normalized to object");
    let mut entries = obj
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    entries.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(PROFILE_ID));

    match applied_profile_id {
        Some(id) => {
            entries.push(json!({
                "id": PROFILE_ID,
                "name": PROFILE_NAME
            }));
            obj.insert("appliedId".to_string(), Value::String(id.to_string()));
        }
        None => {
            let should_clear_applied = obj
                .get("appliedId")
                .and_then(Value::as_str)
                .is_some_and(|id| id == PROFILE_ID);
            if should_clear_applied {
                if let Some(next_id) = entries
                    .iter()
                    .find_map(|entry| entry.get("id").and_then(Value::as_str))
                {
                    obj.insert("appliedId".to_string(), Value::String(next_id.to_string()));
                } else {
                    obj.remove("appliedId");
                }
            }
        }
    }

    obj.insert("entries".to_string(), Value::Array(entries));
    write_json_file(path, &value)
}

fn read_applied_id(path: &Path) -> Option<String> {
    read_json_or_empty(path).ok().and_then(|value| {
        value
            .get("appliedId")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn meta_has_profile_entry(path: &Path) -> bool {
    read_json_or_empty(path)
        .ok()
        .and_then(|value| value.get("entries").and_then(Value::as_array).cloned())
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.get("id").and_then(Value::as_str) == Some(PROFILE_ID))
        })
}

fn is_supported_platform() -> bool {
    cfg!(any(target_os = "macos", windows, target_os = "linux"))
}

#[allow(clippy::needless_return)]
fn current_platform_paths() -> Result<ClaudeDesktopPaths, AppError> {
    #[cfg(target_os = "macos")]
    {
        return Ok(macos_paths_from_home(&get_home_dir()));
    }

    #[cfg(windows)]
    {
        let local_app_data = windows_local_app_data_dir();
        return Ok(windows_paths_from_local_app_data(&local_app_data));
    }

    #[cfg(target_os = "linux")]
    {
        return Ok(linux_paths_from_config_dir(&linux_config_dir()));
    }

    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        Err(unsupported_platform_error())
    }
}

/// Flatpak keeps an app's XDG_CONFIG_HOME inside its sandbox. Claude Desktop
/// installed natively uses the host's ~/.config by default, which must be
/// exposed through the Flatpak filesystem permissions.
///
/// This is intentionally the host *default* configuration directory. We do
/// not expose a directory override or attempt to recover a host-custom
/// XDG_CONFIG_HOME: Flatpak replaces that variable with its private path, so
/// its original host value is not available reliably from the sandbox. Users
/// with a custom host XDG_CONFIG_HOME should run the native CC Switch package.
#[cfg(target_os = "linux")]
fn linux_config_dir() -> PathBuf {
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    linux_config_dir_from_home(&get_home_dir(), xdg_config_home.as_deref(), is_flatpak())
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn linux_config_dir_from_home(
    home: &Path,
    xdg_config_home: Option<&Path>,
    running_in_flatpak: bool,
) -> PathBuf {
    if running_in_flatpak {
        return home.join(".config");
    }

    xdg_config_home
        .filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".config"))
}

#[cfg(target_os = "linux")]
fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").is_file()
}

#[cfg(target_os = "linux")]
fn linux_paths_from_config_dir(config_dir: &Path) -> ClaudeDesktopPaths {
    paths_from_dirs(config_dir.join("Claude"), config_dir.join("Claude-3p"))
}

#[cfg(target_os = "macos")]
fn macos_paths_from_home(home: &Path) -> ClaudeDesktopPaths {
    let app_support = home.join("Library").join("Application Support");
    paths_from_dirs(app_support.join("Claude"), app_support.join("Claude-3p"))
}

#[cfg(windows)]
fn windows_local_app_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| get_home_dir().join("AppData").join("Local"))
}

#[cfg(windows)]
fn windows_paths_from_local_app_data(local_app_data: &Path) -> ClaudeDesktopPaths {
    let normal_dir = pick_windows_claude_dir(local_app_data, false)
        .unwrap_or_else(|| local_app_data.join("Claude"));
    let threep_dir = pick_windows_claude_dir(local_app_data, true)
        .unwrap_or_else(|| local_app_data.join("Claude-3p"));
    paths_from_dirs(normal_dir, threep_dir)
}

#[cfg(windows)]
fn pick_windows_claude_dir(local_app_data: &Path, threep: bool) -> Option<PathBuf> {
    let exact_name = if threep { "Claude-3p" } else { "Claude" };
    let exact = local_app_data.join(exact_name);
    if exact.exists() {
        return Some(exact);
    }

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(local_app_data)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                return false;
            };
            let starts = name.starts_with("Claude");
            let is_threep = name.contains("-3p");
            starts && is_threep == threep
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

#[cfg(any(target_os = "macos", windows, target_os = "linux", test))]
fn paths_from_dirs(normal_dir: PathBuf, threep_dir: PathBuf) -> ClaudeDesktopPaths {
    let config_library_path = threep_dir.join(CONFIG_LIBRARY_DIR);
    let profile_path = config_library_path.join(format!("{PROFILE_ID}.json"));
    let meta_path = config_library_path.join("_meta.json");

    ClaudeDesktopPaths {
        normal_config_path: normal_dir.join(CONFIG_FILE),
        threep_config_path: threep_dir.join(CONFIG_FILE),
        config_library_path,
        profile_path,
        meta_path,
    }
}

/// 代理 origin 根（`http://host:port`），由运行期代理配置的监听地址/端口派生。
/// 监听通配地址会被改写成回环地址——客户端无法连接 `0.0.0.0` / `::`；IPv6 字面量
/// 需补方括号才能进 URL。Desktop 网关与 Codex 聚合 seed 两处调用方共享此派生，
/// 避免各自手写 host 归一化后漂移。
///
/// 注意：`services/proxy.rs` 的 `build_proxy_urls` 里尚存一份**独立的同源推导**
/// （通配地址归一化 + IPv6 方括号，逻辑等价且多了一段运行期端口回填），尚未迁移
/// 到本函数——改动它不在本轮范围内。
pub(crate) fn proxy_origin_from_parts(listen_address: &str, listen_port: u16) -> String {
    let connect_host = match listen_address {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        value => value,
    };
    let connect_host_for_url = if connect_host.contains(':') && !connect_host.starts_with('[') {
        format!("[{connect_host}]")
    } else {
        connect_host.to_string()
    };

    format!("http://{}:{}", connect_host_for_url, listen_port)
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn unsupported_platform_error() -> AppError {
    AppError::localized(
        "claude_desktop.unsupported_platform",
        "当前平台暂不支持 Claude Desktop 3P 配置。支持的平台：macOS、Windows 和 Linux。",
        "Claude Desktop 3P configuration is not supported on this platform yet. Supported platforms: macOS, Windows, and Linux.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::provider::{ClaudeDesktopModelRoute, ProviderMeta};
    use crate::settings::ClaudeDesktopDisplaySettings;
    use serde_json::json;
    use serial_test::serial;
    use std::env;
    use tempfile::TempDir;

    /// 把 HOME/USERPROFILE/LOCALAPPDATA/CC_SWITCH_TEST_HOME 全部钉到临时目录：
    /// `apply_provider` 走的是 `current_platform_paths()`，会写真实的 3P profile 与
    /// `~/.claude/settings.json`，测试里必须整体搬家（2026-09-22 事故口径）。
    struct TempHome {
        #[allow(dead_code)]
        dir: TempDir,
        original_home: Option<String>,
        #[cfg(windows)]
        original_local_app_data: Option<String>,
        original_userprofile: Option<String>,
        original_test_home: Option<String>,
        #[cfg(target_os = "linux")]
        original_xdg_config_home: Option<std::ffi::OsString>,
    }

    impl TempHome {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed to create temp home");
            let original_home = env::var("HOME").ok();
            #[cfg(windows)]
            let original_local_app_data = env::var("LOCALAPPDATA").ok();
            let original_userprofile = env::var("USERPROFILE").ok();
            let original_test_home = env::var("CC_SWITCH_TEST_HOME").ok();
            #[cfg(target_os = "linux")]
            let original_xdg_config_home = env::var_os("XDG_CONFIG_HOME");

            env::set_var("HOME", dir.path());
            #[cfg(windows)]
            env::set_var("LOCALAPPDATA", dir.path().join("AppData").join("Local"));
            env::set_var("USERPROFILE", dir.path());
            env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            // Claude Desktop Linux 路径跟随 XDG_CONFIG_HOME，必须一并钉住
            #[cfg(target_os = "linux")]
            env::remove_var("XDG_CONFIG_HOME");

            Self {
                dir,
                original_home,
                #[cfg(windows)]
                original_local_app_data,
                original_userprofile,
                original_test_home,
                #[cfg(target_os = "linux")]
                original_xdg_config_home,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.original_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }

            #[cfg(windows)]
            {
                match &self.original_local_app_data {
                    Some(value) => env::set_var("LOCALAPPDATA", value),
                    None => env::remove_var("LOCALAPPDATA"),
                }
            }

            match &self.original_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }

            match &self.original_test_home {
                Some(value) => env::set_var("CC_SWITCH_TEST_HOME", value),
                None => env::remove_var("CC_SWITCH_TEST_HOME"),
            }

            #[cfg(target_os = "linux")]
            {
                match &self.original_xdg_config_home {
                    Some(value) => env::set_var("XDG_CONFIG_HOME", value),
                    None => env::remove_var("XDG_CONFIG_HOME"),
                }
            }
        }
    }

    fn test_paths(home: &Path) -> ClaudeDesktopPaths {
        paths_from_dirs(
            home.join("Library")
                .join("Application Support")
                .join("Claude"),
            home.join("Library")
                .join("Application Support")
                .join("Claude-3p"),
        )
    }

    #[cfg(any(target_os = "linux", all(test, unix)))]
    #[test]
    fn linux_config_dir_uses_absolute_xdg_config_home_outside_flatpak() {
        let home = Path::new("/home/tester");
        let xdg = Path::new("/mnt/config");

        assert_eq!(
            linux_config_dir_from_home(home, Some(xdg), false),
            PathBuf::from("/mnt/config")
        );
    }

    #[cfg(any(target_os = "linux", all(test, unix)))]
    #[test]
    fn linux_config_dir_falls_back_for_missing_or_relative_xdg_config_home() {
        let home = Path::new("/home/tester");

        assert_eq!(
            linux_config_dir_from_home(home, None, false),
            PathBuf::from("/home/tester/.config")
        );
        assert_eq!(
            linux_config_dir_from_home(home, Some(Path::new("relative/config")), false),
            PathBuf::from("/home/tester/.config")
        );
    }

    #[cfg(any(target_os = "linux", all(test, unix)))]
    #[test]
    fn linux_config_dir_uses_host_config_when_cc_switch_runs_in_flatpak() {
        let home = Path::new("/home/tester");
        let private_xdg = Path::new("/home/tester/.var/app/com.ccswitch.desktop/config");

        assert_eq!(
            linux_config_dir_from_home(home, Some(private_xdg), true),
            PathBuf::from("/home/tester/.config")
        );
    }

    fn test_db() -> Database {
        Database::memory().expect("memory db")
    }

    fn set_proxy_port(db: &Database, port: u16) {
        let config = crate::proxy::types::ProxyConfig {
            listen_port: port,
            ..Default::default()
        };
        futures::executor::block_on(db.update_proxy_config(config)).expect("update proxy config");
    }

    fn direct_provider(id: &str) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            "Direct".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://gateway.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "test-token",
                    "ANTHROPIC_MODEL": "ignored-by-desktop"
                }
            }),
            Some("https://example.com".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            api_format: Some("anthropic".to_string()),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn proxy_gateway_base_url_rejects_unresolved_ephemeral_port() {
        let db = test_db();
        set_proxy_port(&db, 0);

        let err = proxy_gateway_base_url_from_db(&db)
            .expect_err("unresolved ephemeral port should not produce a :0 URL");
        assert!(
            err.to_string().contains("真实监听端口"),
            "unexpected error: {err}"
        );
    }

    fn official_provider() -> Provider {
        let mut provider = Provider::with_id(
            CLAUDE_DESKTOP_OFFICIAL_PROVIDER_ID.to_string(),
            "Claude Desktop Official".to_string(),
            json!({"env": {}}),
            Some("https://claude.ai/download".to_string()),
        );
        provider.category = Some("official".to_string());
        provider
    }

    fn proxy_provider(id: &str) -> Provider {
        let mut provider = direct_provider(id);
        provider.name = "Proxy".to_string();
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some("openai_chat".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "kimi-k2".to_string(),
                    label_override: Some("Kimi K2".to_string()),
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    fn mimo_anthropic_proxy_provider(id: &str) -> Provider {
        let mut provider = direct_provider(id);
        provider.name = "MiMo Proxy".to_string();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "test-token"
            }
        });
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some("anthropic".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "mimo-v2.5-pro".to_string(),
                    label_override: Some("MiMo v2.5 Pro".to_string()),
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    fn oauth_proxy_provider(id: &str, provider_type: &str, api_format: &str) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            "OAuth Proxy".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://oauth-upstream.example.com"
                }
            }),
            Some("https://example.com".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some(api_format.to_string()),
            provider_type: Some(provider_type.to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "gpt-5.4".to_string(),
                    label_override: Some("GPT-5.4".to_string()),
                    supports_1m: Some(false),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    fn direct_provider_with_models(id: &str) -> Provider {
        let mut provider = direct_provider(id);
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Direct),
            api_format: Some("anthropic".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "claude-sonnet-4-6".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            )]),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn claude_desktop_apply_writes_3p_profile_and_meta() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider("direct");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");

        assert_eq!(normal["deploymentMode"], json!("3p"));
        assert_eq!(threep["deploymentMode"], json!("3p"));
        assert_eq!(profile["inferenceProvider"], json!("gateway"));
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("https://gateway.example.com")
        );
        assert_eq!(profile["inferenceGatewayApiKey"], json!("test-token"));
        assert_eq!(profile["inferenceGatewayAuthScheme"], json!("bearer"));
        assert_eq!(profile["disableDeploymentModeChooser"], json!(true));
        assert_eq!(profile["coworkEgressAllowedHosts"], json!(["*"]));
        assert!(profile.get("inferenceModels").is_none());
        assert_eq!(meta["appliedId"], json!(PROFILE_ID));
        assert!(meta["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|entry| entry["id"] == json!(PROFILE_ID) && entry["name"] == json!(PROFILE_NAME)));
    }

    #[test]
    fn claude_desktop_apply_preserves_non_gateway_profile_fields() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());

        // Simulate a profile Claude Desktop / the user already wrote, carrying
        // non-gateway fields that cc-switch does not own.
        let existing = json!({
            "inferenceGatewayBaseUrl": "https://old.example.com",
            "inferenceGatewayApiKey": "old-token",
            "chatTabEnabled": true,
            "autoModeEnabled": true,
            "inferenceCredentialKind": "static",
            "managedMcpServers": [
                { "name": "opencli", "url": "http://127.0.0.1:31337/" }
            ]
        });
        write_json_file(&paths.profile_path, &existing).expect("pre-write profile");

        let provider = direct_provider("direct");
        let db = test_db();
        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");

        // Gateway fields are refreshed to the new provider's values.
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("https://gateway.example.com")
        );
        assert_eq!(profile["inferenceGatewayApiKey"], json!("test-token"));
        assert_eq!(profile["inferenceProvider"], json!("gateway"));

        // Non-gateway fields are preserved across the switch.
        assert_eq!(profile["chatTabEnabled"], json!(true));
        assert_eq!(profile["autoModeEnabled"], json!(true));
        assert_eq!(profile["inferenceCredentialKind"], json!("static"));
        assert_eq!(profile["managedMcpServers"][0]["name"], json!("opencli"));
    }

    #[test]
    fn claude_desktop_apply_clears_stale_inference_models_when_new_provider_has_none() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());

        // Previous provider carried model mappings; new provider (direct_provider
        // with no model routes) must not leak them.
        let existing = json!({
            "inferenceGatewayBaseUrl": "https://old.example.com",
            "inferenceModels": [
                { "name": "claude-sonnet-4-6", "labelOverride": "old-label" }
            ]
        });
        write_json_file(&paths.profile_path, &existing).expect("pre-write profile");

        let provider = direct_provider("direct");
        let db = test_db();
        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert!(profile.get("inferenceModels").is_none());
    }

    #[test]
    fn claude_desktop_direct_can_write_optional_safe_model_ids() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider_with_models("direct-models");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("https://gateway.example.com")
        );
        assert_eq!(
            profile["inferenceModels"],
            json!([{ "name": "claude-sonnet-4-6", "supports1m": true }])
        );
    }

    #[test]
    fn claude_desktop_direct_rejects_model_mapping_to_non_claude_upstream() {
        let mut provider = direct_provider_with_models("direct-non-claude");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes
            .get_mut("claude-sonnet-4-6")
            .expect("route")
            .model = "mimo-v2.5-pro".to_string();

        let err = validate_provider(&provider).expect_err("direct mapping should fail");
        assert!(err.to_string().contains("本地路由模式"));
    }

    #[test]
    fn claude_desktop_proxy_apply_writes_local_gateway_profile_with_safe_models() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = proxy_provider("proxy");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply proxy provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("http://127.0.0.1:15721/claude-desktop")
        );
        assert_eq!(profile["inferenceGatewayAuthScheme"], json!("bearer"));
        assert_eq!(profile["coworkEgressAllowedHosts"], json!(["*"]));
        assert_ne!(profile["inferenceGatewayApiKey"], json!("test-token"));
        assert!(profile["inferenceGatewayApiKey"]
            .as_str()
            .expect("gateway token")
            .starts_with("ccs-"));
        assert_eq!(
            profile["inferenceModels"],
            json!([{ "name": "claude-sonnet-4-6", "labelOverride": "Kimi K2", "supports1m": true }])
        );
        assert!(!profile.to_string().contains("kimi-k2"));
    }

    #[test]
    fn claude_desktop_proxy_accepts_managed_oauth_providers_without_static_key() {
        for (provider_type, api_format) in [
            ("github_copilot", "openai_chat"),
            ("codex_oauth", "openai_responses"),
            ("xai_oauth", "openai_responses"),
        ] {
            let provider = oauth_proxy_provider(provider_type, provider_type, api_format);
            validate_proxy_provider(&provider).expect("oauth proxy provider should validate");

            let temp = TempDir::new().expect("tempdir");
            let paths = test_paths(temp.path());
            let db = test_db();
            apply_provider_to_paths(&db, &provider, &paths).expect("apply oauth proxy provider");

            let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
            assert_eq!(
                profile["inferenceGatewayBaseUrl"],
                json!("http://127.0.0.1:15721/claude-desktop")
            );
            assert_eq!(
                profile["inferenceModels"],
                json!([{ "name": "claude-sonnet-4-6", "labelOverride": "GPT-5.4" }])
            );
        }
    }

    #[test]
    fn claude_desktop_proxy_maps_known_route_and_rejects_unknown_route() {
        let provider = proxy_provider("proxy");

        let mapped = map_proxy_request_model(
            json!({"model": "claude-sonnet-4-6", "messages": []}),
            &provider,
        )
        .expect("map route");
        assert_eq!(mapped["model"], json!("kimi-k2"));

        let models = model_list_response(&provider).expect("model list");
        assert_eq!(models["data"][0]["id"], json!("claude-sonnet-4-6"));
        assert_eq!(models["data"][0]["supports1m"], json!(true));

        let err = map_proxy_request_model(json!({"model": "claude-opus-4-8"}), &provider)
            .expect_err("unknown route should fail");
        assert!(err.to_string().contains("claude-opus-4-8"));
    }

    #[test]
    fn claude_desktop_proxy_maps_dated_role_alias_via_keyword() {
        // 复现反馈：Claude Desktop 子 agent 请求带发布日期后缀的完整官方名
        // （claude-haiku-4-5-20251001），与 manifest 的简短 route_id（claude-haiku-4-5）
        // 不精确相等，旧逻辑会报 route_unknown。角色关键词回落应将其映射到 Haiku 档。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-pro".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-pro".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-haiku-4-5".to_string(),
                ClaudeDesktopModelRoute {
                    model: "deepseek-v4-flash".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        let mapped = map_proxy_request_model(
            json!({"model": "claude-haiku-4-5-20251001", "messages": []}),
            &provider,
        )
        .expect("dated Haiku alias should map via role keyword");
        assert_eq!(mapped["model"], json!("deepseek-v4-flash"));

        let mapped_sonnet = map_proxy_request_model(
            json!({"model": "claude-sonnet-4-5-20250101", "messages": []}),
            &provider,
        )
        .expect("dated Sonnet alias should map via role keyword");
        assert_eq!(mapped_sonnet["model"], json!("deepseek-v4-pro"));

        // 不含任何角色关键词的模型仍然报错，避免被误映射。
        let err = map_proxy_request_model(json!({"model": "gpt-5"}), &provider)
            .expect_err("model without a role keyword should still fail");
        assert!(err.to_string().contains("gpt-5"));
    }

    #[test]
    fn claude_desktop_proxy_maps_fable_to_opus_tier() {
        // issue #4026/#4049：老用户只配 Sonnet/Opus/Haiku 三档、未显式配置
        // fable 档时，fable 请求按官方分类器降级方向回落到 opus 档兜底。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-opus".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-sonnet".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        let mapped = map_proxy_request_model(
            json!({"model": "claude-fable-5", "messages": []}),
            &provider,
        )
        .expect("fable should fall back to the opus tier");
        assert_eq!(mapped["model"], json!("upstream-opus"));

        // 带 [1m] 标记与日期后缀的形态也应命中同一回落。
        let mapped_one_m = map_proxy_request_model(
            json!({"model": "claude-fable-5[1m]", "messages": []}),
            &provider,
        )
        .expect("fable with [1m] marker should fall back to the opus tier");
        assert_eq!(mapped_one_m["model"], json!("upstream-opus"));

        let mapped_dated = map_proxy_request_model(
            json!({"model": "claude-fable-5-20260609", "messages": []}),
            &provider,
        )
        .expect("dated fable alias should fall back to the opus tier");
        assert_eq!(mapped_dated["model"], json!("upstream-opus"));
    }

    #[test]
    fn claude_desktop_proxy_fable_without_opus_route_still_errors() {
        // 没有 opus 档可回落时保持精确报错语义，不静默落到其他档。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([(
            "claude-sonnet-4-6".to_string(),
            ClaudeDesktopModelRoute {
                model: "upstream-sonnet".to_string(),
                label_override: None,
                supports_1m: Some(true),
            },
        )]);

        let err = map_proxy_request_model(
            json!({"model": "claude-fable-5", "messages": []}),
            &provider,
        )
        .expect_err("fable without an opus route should fail");
        assert!(err.to_string().contains("claude-fable-5"));
    }

    #[test]
    fn claude_desktop_proxy_maps_fable_to_dedicated_route() {
        // Desktop 1.12603.1+ fail-all 校验已放行 claude-fable-5，用户可显式配置
        // 独立 fable 档；此时 fable 请求精确命中 fable 档，不再降级到 opus。
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-opus".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-fable-5".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-fable".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        // 精确匹配优先命中 fable 档
        let mapped = map_proxy_request_model(
            json!({"model": "claude-fable-5", "messages": []}),
            &provider,
        )
        .expect("explicit fable route should match");
        assert_eq!(mapped["model"], json!("upstream-fable"));

        // 带日期后缀经角色关键词回落仍归 fable 档，而非降级 opus
        let mapped_dated = map_proxy_request_model(
            json!({"model": "claude-fable-5-20260609", "messages": []}),
            &provider,
        )
        .expect("dated fable alias should map via fable role keyword");
        assert_eq!(mapped_dated["model"], json!("upstream-fable"));
    }

    #[test]
    fn claude_desktop_proxy_accepts_opus_4_7_4_8_alias_during_rollout() {
        let mut provider = proxy_provider("proxy");
        let current_routes = std::collections::HashMap::from([(
            "claude-opus-4-8".to_string(),
            ClaudeDesktopModelRoute {
                model: "upstream-opus-new".to_string(),
                label_override: None,
                supports_1m: Some(true),
            },
        )]);
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = current_routes;

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-7", "messages": []}),
            &provider,
        )
        .expect("legacy Opus route should map to current route");
        assert_eq!(mapped["model"], json!("upstream-opus-new"));

        let legacy_routes = std::collections::HashMap::from([(
            "claude-opus-4-7".to_string(),
            ClaudeDesktopModelRoute {
                model: "upstream-opus-legacy".to_string(),
                label_override: None,
                supports_1m: Some(true),
            },
        )]);
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = legacy_routes;

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-8", "messages": []}),
            &provider,
        )
        .expect("current Opus route should map to legacy saved route");
        assert_eq!(mapped["model"], json!("upstream-opus-legacy"));
    }

    #[test]
    fn claude_desktop_mimo_anthropic_rewrites_redacted_thinking_for_tool_history() {
        let provider = mimo_anthropic_proxy_provider("mimo");

        let mapped = map_proxy_request_model(
            json!({
                "model": "claude-sonnet-4-6",
                "messages": [{
                    "role": "assistant",
                    "content": [
                        {"type": "redacted_thinking", "data": "opaque"},
                        {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                }]
            }),
            &provider,
        )
        .expect("map MiMo route");

        assert_eq!(mapped["model"], json!("mimo-v2.5-pro"));
        assert_eq!(
            mapped["messages"][0]["content"][0]["type"],
            json!("thinking")
        );
        assert_eq!(
            mapped["messages"][0]["content"][0]["thinking"],
            json!("[redacted thinking]")
        );
        assert_eq!(
            mapped["messages"][0]["content"][1]["type"],
            json!("tool_use")
        );
    }

    #[test]
    fn claude_desktop_mimo_anthropic_injects_thinking_for_tool_history_without_one() {
        let provider = mimo_anthropic_proxy_provider("mimo");

        let mapped = map_proxy_request_model(
            json!({
                "model": "claude-sonnet-4-6",
                "messages": [{
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                }]
            }),
            &provider,
        )
        .expect("map MiMo route");

        assert_eq!(
            mapped["messages"][0]["content"][0]["type"],
            json!("thinking")
        );
        assert_eq!(
            mapped["messages"][0]["content"][0]["thinking"],
            json!("tool call")
        );
        assert_eq!(
            mapped["messages"][0]["content"][1]["type"],
            json!("tool_use")
        );
    }

    #[test]
    fn claude_desktop_mimo_anthropic_keeps_thinking_text_but_drops_signature() {
        let provider = mimo_anthropic_proxy_provider("mimo");

        let mapped = map_proxy_request_model(
            json!({
                "model": "claude-sonnet-4-6",
                "messages": [{
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "Need to inspect the file.", "signature": "anthropic-signature"},
                        {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                }]
            }),
            &provider,
        )
        .expect("map MiMo route");

        assert_eq!(
            mapped["messages"][0]["content"][0]["thinking"],
            json!("Need to inspect the file.")
        );
        assert!(mapped["messages"][0]["content"][0]
            .get("signature")
            .is_none());
    }

    #[test]
    fn claude_desktop_proxy_repairs_legacy_unsafe_route_without_colliding() {
        let mut provider = proxy_provider("proxy");
        provider.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            api_format: Some("openai_chat".to_string()),
            claude_desktop_model_routes: std::collections::HashMap::from([
                (
                    "claude-deepseek-v4-pro".to_string(),
                    ClaudeDesktopModelRoute {
                        model: "deepseek-v4-pro".to_string(),
                        label_override: None,
                        supports_1m: Some(true),
                    },
                ),
                (
                    "claude-old".to_string(),
                    ClaudeDesktopModelRoute {
                        model: "legacy-upstream".to_string(),
                        label_override: None,
                        supports_1m: Some(false),
                    },
                ),
                (
                    "claude-sonnet-5".to_string(),
                    ClaudeDesktopModelRoute {
                        model: "claude-sonnet-5".to_string(),
                        label_override: None,
                        supports_1m: Some(false),
                    },
                ),
            ]),
            ..Default::default()
        });

        let routes = proxy_model_routes(&provider).expect("routes");
        assert_eq!(routes.len(), 3);
        let repaired = routes
            .iter()
            .find(|route| route.upstream_model == "deepseek-v4-pro")
            .expect("repaired route");
        assert_eq!(repaired.route_id, "claude-opus-5");
        assert_eq!(repaired.label_override.as_deref(), Some("deepseek-v4-pro"));
        assert!(repaired.supports_1m);
        let repaired_old = routes
            .iter()
            .find(|route| route.upstream_model == "legacy-upstream")
            .expect("legacy route should be repaired");
        assert_eq!(repaired_old.route_id, "claude-haiku-4-5");
        assert_eq!(
            repaired_old.label_override.as_deref(),
            Some("legacy-upstream")
        );

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-8", "messages": []}),
            &provider,
        )
        .expect("map repaired route");
        assert_eq!(mapped["model"], json!("deepseek-v4-pro"));

        let legacy_mapped =
            map_proxy_request_model(json!({"model": "claude-old", "messages": []}), &provider)
                .expect("map stale profile route");
        assert_eq!(legacy_mapped["model"], json!("legacy-upstream"));
    }

    #[test]
    fn claude_desktop_proxy_strips_1m_suffix_before_route_lookup() {
        let mut provider = proxy_provider("proxy");
        provider
            .meta
            .as_mut()
            .expect("meta")
            .claude_desktop_model_routes = std::collections::HashMap::from([
            (
                "claude-sonnet-4-6".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-sonnet".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
            (
                "claude-opus-4-8".to_string(),
                ClaudeDesktopModelRoute {
                    model: "upstream-opus".to_string(),
                    label_override: None,
                    supports_1m: Some(true),
                },
            ),
        ]);

        let mapped = map_proxy_request_model(
            json!({"model": "claude-opus-4-8[1m]", "messages": []}),
            &provider,
        )
        .expect("compact 1M suffix should map to Opus route");
        assert_eq!(mapped["model"], json!("upstream-opus"));

        let mapped = map_proxy_request_model(
            json!({"model": "claude-sonnet-4-6 [1M]", "messages": []}),
            &provider,
        )
        .expect("spaced uppercase 1M suffix should map to Sonnet route");
        assert_eq!(mapped["model"], json!("upstream-sonnet"));

        let err = map_proxy_request_model(json!({"model": "gpt-5[1m]", "messages": []}), &provider)
            .expect_err("non-Claude route should still fail after stripping 1M suffix");
        assert!(err.to_string().contains("gpt-5[1m]"));
    }

    #[test]
    fn claude_desktop_rejects_1m_suffix_as_model_id() {
        assert!(!is_claude_safe_model_id("claude-sonnet-4-6 [1m]"));
        assert!(!is_claude_safe_model_id("  claude-sonnet-4-6  [1M]  "));
        assert!(!is_claude_safe_model_id("claude-old"));
        assert!(!is_claude_safe_model_id("claude-3-5-sonnet-20241022"));
        assert!(!is_claude_safe_model_id("claude-deepseek-v4-pro"));
        assert!(!is_claude_safe_model_id("claude-gpt-5-4"));
        assert!(!is_claude_safe_model_id("claude-"));
        assert!(!is_claude_safe_model_id("anthropic/claude-"));
        assert!(!is_claude_safe_model_id("sonnet"));
        assert!(!is_claude_safe_model_id("sonnet-"));
        // 角色前缀后无实际标识的退化值必须拒绝
        assert!(!is_claude_safe_model_id("claude-sonnet-"));
        assert!(!is_claude_safe_model_id("claude-opus-"));
        assert!(!is_claude_safe_model_id("anthropic/claude-haiku-"));
        assert!(is_claude_safe_model_id("  claude-sonnet-4-6  "));
        assert!(is_claude_safe_model_id("anthropic/claude-opus-4-8"));
    }

    #[test]
    fn claude_desktop_rejects_vendor_tokens_in_model_id() {
        // 厂商词黑名单（app.asar 内 Vxe）。角色前缀能过、厂商词不能过——
        // 这正是旧槽位 ID 方案（claude-{档位}-{供应商名 slug}）踩的坑：
        // `claude-fable-deepseek` 的 tail 是 `fable-deepseek`，角色前缀规则放行，
        // 但 Claude Desktop 会判其 "is not an Anthropic model" 并从
        // inferenceModels 移除，且**整组生效**（2026-09-22 实测：4 个槽位被删光、
        // 选择器变空）。故这一层必须与角色前缀规则并列存在。
        assert!(!is_claude_safe_model_id("claude-fable-deepseek"));
        assert!(!is_claude_safe_model_id("claude-fable-zhipu-glm"));
        assert!(!is_claude_safe_model_id("claude-opus-deepseek"));
        assert!(!is_claude_safe_model_id("claude-sonnet-1-glm"));
        assert!(!is_claude_safe_model_id("claude-haiku-kimi"));
        assert!(!is_claude_safe_model_id("anthropic/claude-sonnet-1-qwen"));
        // 大小写不敏感（与 Vxe 前的 toLowerCase 一致）
        assert!(!is_claude_safe_model_id("claude-sonnet-DeepSeek"));
        // 词边界类厂商词只在独立成词时命中：`ling` 不该误伤 `lingua`
        assert!(is_claude_safe_model_id("claude-sonnet-lingua"));
        // 新方案产物（档位 + 序号）不受影响
        assert!(is_claude_safe_model_id("claude-sonnet-1"));
        assert!(is_claude_safe_model_id("claude-fable-2"));
    }

    #[test]
    fn claude_desktop_apply_rolls_back_when_profile_write_fails() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider("direct");
        let db = test_db();

        write_json_file(
            &paths.normal_config_path,
            &json!({"deploymentMode": "1p", "normal": true}),
        )
        .expect("write normal");
        write_json_file(
            &paths.threep_config_path,
            &json!({"deploymentMode": "1p", "threep": true}),
        )
        .expect("write 3p");
        fs::write(&paths.config_library_path, "not a directory").expect("block profile parent");

        apply_provider_to_paths(&db, &provider, &paths).expect_err("apply should fail");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");

        assert_eq!(normal, json!({"deploymentMode": "1p", "normal": true}));
        assert_eq!(threep, json!({"deploymentMode": "1p", "threep": true}));
        assert!(!paths.profile_path.exists());
    }

    #[test]
    fn claude_desktop_write_meta_recovers_non_object_meta_file() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        if let Some(parent) = paths.meta_path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&paths.meta_path, "[]").expect("write invalid meta shape");

        write_meta(&paths.meta_path, Some(PROFILE_ID)).expect("write meta");

        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");
        assert_eq!(meta["appliedId"], json!(PROFILE_ID));
        assert!(meta["entries"].as_array().is_some());
    }

    #[test]
    fn claude_desktop_restore_switches_to_1p_and_removes_cc_switch_profile() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let provider = direct_provider("direct");
        let db = test_db();

        apply_provider_to_paths(&db, &provider, &paths).expect("apply provider");
        restore_official_at_paths(&paths).expect("restore official");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");

        assert_eq!(normal["deploymentMode"], json!("1p"));
        assert_eq!(threep["deploymentMode"], json!("1p"));
        assert!(!paths.profile_path.exists());
        assert!(meta.get("appliedId").is_none());
        assert!(!meta["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|entry| entry["id"] == json!(PROFILE_ID)));
    }

    #[test]
    fn claude_desktop_official_provider_restores_1p_mode() {
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let direct = direct_provider("direct");
        let db = test_db();

        apply_provider_to_paths(&db, &direct, &paths).expect("apply direct provider");
        apply_provider_to_paths(&db, &official_provider(), &paths)
            .expect("restore official provider");

        let normal: Value = read_json_file(&paths.normal_config_path).expect("read normal config");
        let threep: Value = read_json_file(&paths.threep_config_path).expect("read 3p config");
        let meta: Value = read_json_file(&paths.meta_path).expect("read meta");

        assert_eq!(normal["deploymentMode"], json!("1p"));
        assert_eq!(threep["deploymentMode"], json!("1p"));
        assert!(!paths.profile_path.exists());
        assert!(meta.get("appliedId").is_none());
    }

    #[test]
    fn claude_desktop_compatibility_filters_non_direct_providers() {
        let direct = direct_provider("direct");
        assert!(is_compatible_direct_provider(&direct));

        let mut claude_official = Provider::with_id(
            "claude-official".to_string(),
            "Claude Official".to_string(),
            json!({"env": {}}),
            Some("https://www.anthropic.com/claude-code".to_string()),
        );
        claude_official.category = Some("official".to_string());
        assert!(!is_compatible_direct_provider(&claude_official));

        let mut openai_format = direct_provider("openai");
        openai_format.meta = Some(ProviderMeta {
            api_format: Some("openai_chat".to_string()),
            ..Default::default()
        });
        assert!(!is_compatible_direct_provider(&openai_format));

        let mut copilot = direct_provider("copilot");
        copilot.meta = Some(ProviderMeta {
            provider_type: Some("github_copilot".to_string()),
            ..Default::default()
        });
        assert!(!is_compatible_direct_provider(&copilot));

        let mut full_url = direct_provider("full_url");
        full_url.meta = Some(ProviderMeta {
            is_full_url: Some(true),
            ..Default::default()
        });
        assert!(!is_compatible_direct_provider(&full_url));

        let missing_bearer = Provider::with_id(
            "x-api-key".to_string(),
            "x-api-key".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://gateway.example.com",
                    "ANTHROPIC_API_KEY": "sk-ant"
                }
            }),
            None,
        );
        assert!(!is_compatible_direct_provider(&missing_bearer));
    }

    #[test]
    fn inject_display_settings_writes_keys_when_configured() {
        let mut profile = json!({ "inferenceProvider": "gateway" });
        let display = ClaudeDesktopDisplaySettings {
            name: "Chris".into(),
            subtitle: "Gateway".into(),
            attribution: false,
        };
        inject_display_settings(&mut profile, Some(&display));
        assert_eq!(profile["deploymentDisplayName"], json!("Chris"));
        assert_eq!(profile["deploymentDisplaySubtitle"], json!("Gateway"));
        assert_eq!(profile["endUserAttribution"], json!(false));
    }

    #[test]
    fn inject_display_settings_omits_keys_when_none() {
        let mut profile = json!({ "inferenceProvider": "gateway" });
        inject_display_settings(&mut profile, None);
        assert!(profile.get("deploymentDisplayName").is_none());
        assert!(profile.get("deploymentDisplaySubtitle").is_none());
        assert!(profile.get("endUserAttribution").is_none());
    }

    #[test]
    fn inject_display_settings_omits_keys_when_name_empty() {
        let mut profile = json!({ "inferenceProvider": "gateway" });
        let display = ClaudeDesktopDisplaySettings {
            name: "".into(),
            subtitle: "Gateway".into(),
            attribution: false,
        };
        inject_display_settings(&mut profile, Some(&display));
        assert!(profile.get("deploymentDisplayName").is_none());
        assert!(profile.get("deploymentDisplaySubtitle").is_none());
        assert!(profile.get("endUserAttribution").is_none());
    }

    #[test]
    fn inject_display_settings_omits_subtitle_when_empty() {
        let mut profile = json!({ "inferenceProvider": "gateway" });
        let display = ClaudeDesktopDisplaySettings {
            name: "Chris".into(),
            subtitle: "".into(),
            attribution: false,
        };
        inject_display_settings(&mut profile, Some(&display));
        assert_eq!(profile["deploymentDisplayName"], json!("Chris"));
        assert!(profile.get("deploymentDisplaySubtitle").is_none());
        assert_eq!(profile["endUserAttribution"], json!(false));
    }

    #[test]
    fn claude_desktop_display_deserializes_partial_object() {
        let v: ClaudeDesktopDisplaySettings =
            serde_json::from_value(json!({ "name": "Chris" })).expect("partial object");
        assert_eq!(v.name, "Chris");
        assert_eq!(v.subtitle, "");
        assert!(!v.attribution);
    }

    #[test]
    fn aggregate_provider_derives_model_routes_from_slots() {
        let mut provider = direct_provider("agg");
        let meta = provider.meta.get_or_insert_with(Default::default);
        meta.aggregate_routes = Some(crate::aggregate::AggregateRoutes {
            slots: vec![
                crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-sonnet-1".into(),
                    tier: crate::aggregate::AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3".into(),
                    label: Some("智谱 GLM-5.3".into()),
                    supports_1m: true,
                    max_effort: None,
                },
                crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-haiku-1".into(),
                    tier: crate::aggregate::AggregateTier::Haiku,
                    provider_id: "p-ds".into(),
                    upstream_model: "deepseek-flash".into(),
                    label: None,
                    supports_1m: false,
                    max_effort: None,
                },
                crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-sonnet-2".into(),
                    tier: crate::aggregate::AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3-flash".into(),
                    label: None,
                    supports_1m: false,
                    max_effort: Some("xhigh".into()),
                },
                crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-sonnet-3".into(),
                    tier: crate::aggregate::AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3-air".into(),
                    label: None,
                    supports_1m: false,
                    max_effort: Some("ultra".into()),
                },
            ],
            default_target: crate::aggregate::DefaultTarget::ProviderId("p-glm".into()),
            default_model: None,
            retired_route_ids: vec![],
            alias_rules: vec![],
        });

        let routes = proxy_model_routes(&provider).expect("routes");
        // 顺序保留 slots 的供应商分组序（2026-09-24 改造，不再按 route_id 字典序）
        assert_eq!(routes.len(), 4);
        assert_eq!(routes[0].route_id, "claude-sonnet-1");
        assert_eq!(routes[0].upstream_model, "glm-5.3");
        assert_eq!(routes[0].label_override.as_deref(), Some("智谱 GLM-5.3"));
        assert!(routes[0].supports_1m);
        assert_eq!(routes[1].route_id, "claude-haiku-1");
        assert_eq!(routes[1].upstream_model, "deepseek-flash");
        assert_eq!(routes[1].label_override, None);
        assert!(!routes[1].supports_1m);
        // maxEffort 白名单：合法值透出，非法值被过滤为 None（不写会让 Desktop 拒收的字段）
        assert_eq!(routes[2].max_effort.as_deref(), Some("xhigh"));
        assert_eq!(routes[3].max_effort, None);

        // 端到端：条目按槽位顺序写入 profile，maxEffort 只随合法值出现在条目对象上
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        apply_provider_to_paths(&test_db(), &provider, &paths).expect("apply aggregate provider");
        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert_eq!(profile["inferenceModels"][2]["maxEffort"], json!("xhigh"));
        assert!(profile["inferenceModels"][3].get("maxEffort").is_none());
    }

    /// 构造一个仅带聚合路由表、`settings_config` 为空对象的供应商（无端点无凭据）。
    fn aggregate_provider_without_credentials(id: &str) -> Provider {
        let mut provider =
            Provider::with_id(id.to_string(), "Aggregate".to_string(), json!({}), None);
        provider.meta = Some(ProviderMeta {
            aggregate_routes: Some(crate::aggregate::AggregateRoutes {
                slots: vec![crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-sonnet-1".into(),
                    tier: crate::aggregate::AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3".into(),
                    label: None,
                    supports_1m: false,
                    max_effort: None,
                }],
                default_target: crate::aggregate::DefaultTarget::ProviderId("p-glm".into()),
                default_model: None,
                retired_route_ids: vec![],
                alias_rules: vec![],
            }),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn validate_direct_provider_accepts_aggregate_without_endpoint_or_credentials() {
        // 聚合供应商自身无端点无凭据（转发时用目标供应商的），故空 settings_config
        // 也必须通过直连校验 —— 不得逼用户编造占位端点与密钥。
        let aggregate = aggregate_provider_without_credentials("agg");
        validate_direct_provider(&aggregate)
            .expect("aggregate provider must validate without endpoint or credentials");
    }

    #[test]
    fn validate_direct_provider_still_rejects_ordinary_provider_without_credentials() {
        // 对照：普通供应商的空 settings_config 仍必须被拒（聚合短路不得顺手放宽它）。
        let plain = Provider::with_id("plain".to_string(), "Plain".to_string(), json!({}), None);
        validate_direct_provider(&plain)
            .expect_err("ordinary provider without endpoint/credentials must still be rejected");
    }

    #[test]
    fn validate_proxy_provider_accepts_aggregate_without_endpoint_or_credentials() {
        // 聚合供应商走本地路由（代理）短路：自身无端点无凭据（转发时用目标供应商的），
        // 故空 settings_config 也必须通过代理校验 —— 这正是 UI 持久化的主路径
        // （meta.claudeDesktopMode = "proxy"）。
        let mut aggregate = aggregate_provider_without_credentials("agg");
        aggregate
            .meta
            .as_mut()
            .expect("meta present")
            .claude_desktop_mode = Some(ClaudeDesktopMode::Proxy);
        validate_proxy_provider(&aggregate)
            .expect("aggregate provider must validate as proxy without endpoint or credentials");
    }

    #[test]
    fn validate_proxy_provider_still_rejects_ordinary_provider_without_credentials() {
        // 对照：普通代理供应商的空 settings_config 仍必须被拒（聚合短路不得顺手放宽它）。
        let mut plain =
            Provider::with_id("plain".to_string(), "Plain".to_string(), json!({}), None);
        plain.meta = Some(ProviderMeta {
            claude_desktop_mode: Some(ClaudeDesktopMode::Proxy),
            ..Default::default()
        });
        validate_proxy_provider(&plain).expect_err(
            "ordinary proxy provider without endpoint/credentials must still be rejected",
        );
    }

    #[test]
    fn apply_aggregate_provider_without_credentials_writes_local_gateway_profile() {
        // 聚合供应商保存后必须能真正启用：空 settings_config 也要走代理分支写 profile
        // （模型列表由槽位派生，网关地址为本地代理），而不是去取聚合自己的端点/凭据。
        let temp = TempDir::new().expect("tempdir");
        let paths = test_paths(temp.path());
        let db = test_db();
        let aggregate = aggregate_provider_without_credentials("agg");

        apply_provider_to_paths(&db, &aggregate, &paths).expect("apply aggregate provider");

        let profile: Value = read_json_file(&paths.profile_path).expect("read profile");
        assert_eq!(
            profile["inferenceGatewayBaseUrl"],
            json!("http://127.0.0.1:15721/claude-desktop")
        );
        assert_eq!(profile["inferenceModels"], json!(["claude-sonnet-1"]));
    }

    #[test]
    fn apply_provider_to_paths_returns_routes_only_for_aggregate_provider() {
        let db = test_db();

        let temp = TempDir::new().expect("tempdir");
        let aggregate = apply_provider_to_paths(
            &db,
            &aggregate_provider_without_credentials("agg"),
            &test_paths(temp.path()),
        )
        .expect("apply aggregate provider");
        // 聚合槽位即 picker 行来源：route_id 必须与槽位一致，否则 CLI 侧点不到目标
        let aggregate_routes = aggregate.expect("aggregate must yield routes");
        assert_eq!(aggregate_routes.len(), 1);
        assert_eq!(aggregate_routes[0].route_id, "claude-sonnet-1");

        let temp = TempDir::new().expect("tempdir");
        // 普通代理供应商（模型映射行）不生成 picker 行：spec 范围只覆盖聚合
        assert!(
            apply_provider_to_paths(&db, &proxy_provider("proxy"), &test_paths(temp.path()))
                .expect("apply proxy provider")
                .is_none()
        );

        let temp = TempDir::new().expect("tempdir");
        assert!(
            apply_provider_to_paths(&db, &direct_provider("direct"), &test_paths(temp.path()))
                .expect("apply direct provider")
                .is_none()
        );

        let temp = TempDir::new().expect("tempdir");
        assert!(
            apply_provider_to_paths(&db, &official_provider(), &test_paths(temp.path()))
                .expect("apply official provider")
                .is_none()
        );
    }

    #[test]
    #[serial]
    fn apply_provider_syncs_model_picker_and_removes_it_for_plain_provider() {
        let _home = TempHome::new();
        let db = test_db();
        let settings_path = crate::config::get_claude_settings_path();
        let aggregate = aggregate_provider_without_credentials("agg");
        let plain = direct_provider("direct");

        apply_provider(&db, &aggregate).expect("apply aggregate provider");
        let saved: Value = read_json_file(&settings_path).expect("read settings");
        assert_eq!(saved["modelPicker"]["replaceBuiltInOptions"], json!(false));
        assert_eq!(
            saved["modelPicker"]["options"][0]["model"],
            json!("claude-sonnet-1")
        );

        // 切到非聚合：本应用写的 picker 键随 profile 一起消失，同批写入的邻键必须留着
        let mut saved: Value = read_json_file(&settings_path).expect("read settings");
        saved["theme"] = json!("dark");
        write_json_file(&settings_path, &saved).expect("seed settings");
        apply_provider(&db, &plain).expect("apply direct provider");
        let saved: Value = read_json_file(&settings_path).expect("read settings");
        assert!(saved.get("modelPicker").is_none(), "{saved}");
        assert_eq!(saved["theme"], json!("dark"));

        // 手写值不可证明是本应用写的 → 不删（D3）
        let handwritten =
            r#"{"theme":"dark","modelPicker":{"options":[{"model":"claude-opus-5"}]}}"#;
        fs::write(&settings_path, handwritten).expect("seed handwritten settings");
        apply_provider(&db, &plain).expect("apply direct provider");
        assert_eq!(
            fs::read_to_string(&settings_path).expect("read settings"),
            handwritten
        );
    }
}
