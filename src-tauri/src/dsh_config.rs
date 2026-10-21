//! DeepSeek Harness 的 cordis.patch.yml / .credentials.yaml 薄适配器。
//!
//! DSH 桌面版的现行配置是每个 profile 下的 `cordis.patch.yml`（loader patch
//! 数组，llm-pi-ai 供应商住在 `id: llm-pi-ai` 条目的 `config.providers` 里）；
//! `$DSH_HOME/settings.yaml` 是旧版遗留的一次性导入源，本模块不写它。
//! cc-switch 只拥有 llm-pi-ai 条目里 `providers.<route>` 的键与
//! `.credentials.yaml refs:<name>` 条目；数组里其余条目与键原样保留。
//! serde_yaml round-trip 不保留注释（spec D2；该文件头部注释为模板说明，可接受）。

use crate::error::AppError;
use serde_json::{json, Value};
use serde_yaml::Mapping;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DshModelEntry {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modalities: Option<DshModelModalities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<DshModelLimit>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DshModelModalities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DshModelLimit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DshProviderSpec {
    pub display_name: String,
    pub api: String,
    pub base_url: String,
    pub api_key_env: String,
    pub models: Vec<DshModelEntry>,
}

const CREDENTIALS_FILE: &str = ".credentials.yaml";
const PROFILES_DIR: &str = "profiles";
const PATCH_FILE: &str = "cordis.patch.yml";
const LLM_ENTRY_ID: &str = "llm-pi-ai";
const LLM_ENTRY_NAME: &str = "@deepseek-ai/dsh-llm-pi-ai";
/// 聚合供应商固定 route 名。
pub const AGGREGATE_ROUTE: &str = "cc-switch";
/// 聚合供应商的凭据引用名（写入 `.credentials.yaml refs:`）。
pub const AGGREGATE_TOKEN_ENV: &str = "CC_SWITCH_GATEWAY_TOKEN";

#[cfg(test)]
static TEST_DSH_PATCH_PATH: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));
#[cfg(test)]
static TEST_DSH_EXTRA_PATCH_PATHS: LazyLock<Mutex<Vec<PathBuf>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn get_dsh_dir() -> Result<PathBuf, AppError> {
    #[cfg(test)]
    if let Some(path) = TEST_DSH_PATCH_PATH.lock().expect("test path").clone() {
        return Ok(path.parent().expect("test parent").to_path_buf());
    }
    if let Some(path) = crate::settings::get_dsh_override_dir() {
        return Ok(path);
    }
    if let Some(env) = std::env::var_os("DSH_HOME") {
        if !env.is_empty() {
            let path = PathBuf::from(&env);
            if !path.is_absolute() {
                return Err(AppError::InvalidInput(
                    "DSH_HOME 必须是绝对路径".to_string(),
                ));
            }
            return Ok(path);
        }
    }
    Ok(crate::config::get_home_dir().join(".dsh"))
}

/// 全部活跃 profile 的 patch 文件路径——凡 profiles/*/ 下有 cordis.yml 的目录
/// 都是活跃 profile（desktop/tui/web/headless/用户自建），同步时全部写入，
/// 任何 DSH 界面都能看到供应商。
fn get_all_patch_paths() -> Result<Vec<PathBuf>, AppError> {
    #[cfg(test)]
    if let Some(path) = TEST_DSH_PATCH_PATH
        .lock()
        .expect("lock DSH test patch path")
        .clone()
    {
        let mut paths = vec![path];
        paths.extend(
            TEST_DSH_EXTRA_PATCH_PATHS
                .lock()
                .expect("lock extra test paths")
                .clone(),
        );
        return Ok(paths);
    }
    let profiles_dir = get_dsh_dir()?.join(PROFILES_DIR);
    if !profiles_dir.exists() {
        return Ok(vec![profiles_dir.join("desktop").join(PATCH_FILE)]);
    }
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(&profiles_dir).map_err(|e| AppError::io(&profiles_dir, e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let dir = entry.path();
        // 有 cordis.yml = 活跃 profile
        if dir.join("cordis.yml").exists() && dir.is_dir() {
            paths.push(dir.join(PATCH_FILE));
        }
    }
    if paths.is_empty() {
        paths.push(profiles_dir.join("desktop").join(PATCH_FILE));
    }
    Ok(paths)
}

/// 读取用：desktop profile 的 patch（规范化视图）。
fn get_patch_path() -> Result<PathBuf, AppError> {
    // 测试态与 get_all_patch_paths 同源：读也走 TEST_DSH_PATCH_PATH，
    // 否则 read_providers 会读真实 ~/.dsh（用户环境决定成败）。
    #[cfg(test)]
    if let Some(path) = TEST_DSH_PATCH_PATH
        .lock()
        .expect("lock DSH test patch path")
        .clone()
    {
        return Ok(path);
    }
    Ok(get_dsh_dir()?
        .join(PROFILES_DIR)
        .join("desktop")
        .join(PATCH_FILE))
}

// ---------- 纯文件层 ----------

fn read_yaml_value(path: &Path, what: &str) -> Result<serde_yaml::Value, AppError> {
    if !path.exists() {
        return Ok(serde_yaml::Value::Null);
    }
    let text = std::fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|e| {
        AppError::Config(format!(
            "DeepSeek Harness {what} 解析失败 ({}): {e}",
            path.display()
        ))
    })?;
    Ok(doc)
}

fn write_yaml_value(path: &Path, doc: &serde_yaml::Value, what: &str) -> Result<(), AppError> {
    let text = serde_yaml::to_string(doc)
        .map_err(|e| AppError::Config(format!("DeepSeek Harness {what} 序列化失败: {e}")))?;
    std::fs::write(path, text).map_err(|e| AppError::io(path, e))
}

fn json_to_yaml(v: &Value) -> serde_yaml::Value {
    serde_yaml::to_value(v).unwrap_or(serde_yaml::Value::Null)
}

fn yaml_kind(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Mapping(_) => "mapping",
        serde_yaml::Value::Sequence(_) => "sequence",
        serde_yaml::Value::String(_) => "string",
        _ => "scalar",
    }
}

/// 读 patch 数组；文件不存在/空 → 空数组。根必须是序列（loader patch 契约）。
fn read_patch_entries(path: &Path) -> Result<Vec<serde_yaml::Value>, AppError> {
    match read_yaml_value(path, "cordis.patch")? {
        serde_yaml::Value::Sequence(entries) => Ok(entries),
        serde_yaml::Value::Null => Ok(Vec::new()),
        other => Err(AppError::Config(format!(
            "DeepSeek Harness cordis.patch 根节点必须是数组，实际是 {}",
            yaml_kind(&other)
        ))),
    }
}

fn is_llm_entry(entry: &serde_yaml::Value) -> bool {
    entry
        .as_mapping()
        .and_then(|m| m.get(serde_yaml::Value::from("id")))
        .and_then(|v| v.as_str())
        == Some(LLM_ENTRY_ID)
}

/// 取（或创建）llm-pi-ai 条目的 config.providers 映射。
fn providers_mut(entries: &mut Vec<serde_yaml::Value>) -> &mut Mapping {
    if !entries.iter().any(is_llm_entry) {
        let mut map = Mapping::new();
        map.insert(
            serde_yaml::Value::from("id"),
            serde_yaml::Value::from(LLM_ENTRY_ID),
        );
        map.insert(
            serde_yaml::Value::from("name"),
            serde_yaml::Value::from(LLM_ENTRY_NAME),
        );
        entries.push(serde_yaml::Value::Mapping(map));
    }
    let entry = entries
        .iter_mut()
        .find(|e| is_llm_entry(e))
        .expect("entry guaranteed above")
        .as_mapping_mut()
        .expect("llm entry is a mapping");
    let config = entry
        .entry(serde_yaml::Value::from("config"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if !config.is_mapping() {
        *config = serde_yaml::Value::Mapping(Mapping::new());
    }
    let config_map = config.as_mapping_mut().expect("just made a mapping");
    let providers = config_map
        .entry(serde_yaml::Value::from("providers"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if !providers.is_mapping() {
        *providers = serde_yaml::Value::Mapping(Mapping::new());
    }
    providers.as_mapping_mut().expect("just made a mapping")
}

fn prune_llm_entry_if_empty(entries: &mut Vec<serde_yaml::Value>) {
    let Some(idx) = entries.iter().position(is_llm_entry) else {
        return;
    };
    let empty = entries[idx]
        .as_mapping()
        .and_then(|m| m.get(serde_yaml::Value::from("config")))
        .and_then(|c| c.get(serde_yaml::Value::from("providers")))
        .and_then(|p| p.as_mapping())
        .is_some_and(|p| p.is_empty());
    if empty {
        entries.remove(idx);
    }
}

fn provider_yaml(spec: &DshProviderSpec) -> serde_yaml::Value {
    json_to_yaml(&json!({
        "displayName": spec.display_name,
        "api": spec.api,
        "baseURL": spec.base_url,
        "apiKeyEnv": spec.api_key_env,
        "models": spec.models
            .iter()
            .map(|m| json!({"id": m.id, "name": m.name}))
            .collect::<Vec<_>>(),
    }))
}

/// 纯路径版：upsert 一个 provider 进 patch 文件。
pub(crate) fn upsert_provider_in_file(
    path: &Path,
    route: &str,
    spec: &DshProviderSpec,
) -> Result<(), AppError> {
    let mut entries = read_patch_entries(path)?;
    providers_mut(&mut entries).insert(serde_yaml::Value::from(route), provider_yaml(spec));
    write_yaml_value(path, &serde_yaml::Value::Sequence(entries), "cordis.patch")
}

/// 纯路径版：移除一个 provider；providers 清空时连 llm-pi-ai 条目一并收走。
pub(crate) fn remove_provider_in_file(path: &Path, route: &str) -> Result<(), AppError> {
    let mut entries = read_patch_entries(path)?;
    let providers = providers_mut(&mut entries);
    remove_protocol_children(providers, route);
    providers.remove(serde_yaml::Value::from(route));
    prune_llm_entry_if_empty(&mut entries);
    write_yaml_value(path, &serde_yaml::Value::Sequence(entries), "cordis.patch")
}

/// Derived protocol routes share the parent's credential reference and ownership.
fn remove_protocol_children(providers: &mut Mapping, route: &str) {
    let env = serde_yaml::Value::from(credential_env_for_route(route));
    for api in [
        "anthropic-messages",
        "openai-responses",
        "openai-completions",
    ] {
        let child = serde_yaml::Value::from(format!("{route}--{api}"));
        if providers
            .get(&child)
            .and_then(|value| value.get("apiKeyEnv"))
            == Some(&env)
        {
            providers.remove(&child);
        }
    }
}

/// 纯路径版：读 providers（保持 patch 顺序）。
pub(crate) fn read_providers_from_file(
    path: &Path,
) -> Result<indexmap::IndexMap<String, Value>, AppError> {
    let entries = read_patch_entries(path)?;
    let mut out = indexmap::IndexMap::new();
    for entry in &entries {
        if !is_llm_entry(entry) {
            continue;
        }
        if let Some(providers) = entry
            .as_mapping()
            .and_then(|m| m.get(serde_yaml::Value::from("config")))
            .and_then(|c| c.get(serde_yaml::Value::from("providers")))
            .and_then(|p| p.as_mapping())
        {
            for (k, v) in providers {
                if let Some(name) = k.as_str() {
                    out.insert(
                        name.to_string(),
                        serde_json::to_value(v).unwrap_or(Value::Null),
                    );
                }
            }
        }
    }
    Ok(out)
}

// ---------- 公共 API（全局路径解析）----------

pub fn read_providers() -> Result<indexmap::IndexMap<String, Value>, AppError> {
    read_providers_from_file(&get_patch_path()?)
}

pub fn upsert_provider(route: &str, spec: &DshProviderSpec) -> Result<(), AppError> {
    for path in get_all_patch_paths()? {
        upsert_provider_in_file(&path, route, spec)?;
    }
    Ok(())
}

pub fn remove_provider(route: &str) -> Result<(), AppError> {
    for path in get_all_patch_paths()? {
        remove_provider_in_file(&path, route)?;
    }
    Ok(())
}

/// 是否已配置过聚合 route（决定「用户未启用 DSH 接入」时 apply 不写文件）。
pub fn aggregate_entry_exists() -> Result<bool, AppError> {
    Ok(read_providers()?.contains_key(AGGREGATE_ROUTE))
}

// ---------- 凭据（.credentials.yaml，目录参数化以便测试）----------

pub fn upsert_credential_ref(dir: &Path, name: &str, secret: &str) -> Result<(), AppError> {
    let path = dir.join(CREDENTIALS_FILE);
    let mut root = match read_yaml_value(&path, "credentials")? {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => Mapping::new(),
        other => {
            return Err(AppError::Config(format!(
                "DeepSeek Harness credentials 根节点必须是映射，实际是 {}",
                yaml_kind(&other)
            )))
        }
    };
    let refs = root
        .entry(serde_yaml::Value::from("refs"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if !refs.is_mapping() {
        *refs = serde_yaml::Value::Mapping(Mapping::new());
    }
    refs.as_mapping_mut().expect("just made a mapping").insert(
        serde_yaml::Value::from(name),
        serde_yaml::Value::from(secret),
    );
    write_yaml_value(&path, &serde_yaml::Value::Mapping(root), "credentials")
}

pub fn remove_credential_ref(
    dir: &Path,
    name: &str,
    only_if_matches: Option<&str>,
) -> Result<(), AppError> {
    let path = dir.join(CREDENTIALS_FILE);
    let mut root = match read_yaml_value(&path, "credentials")? {
        serde_yaml::Value::Mapping(m) => m,
        _ => return Ok(()),
    };
    if let Some(serde_yaml::Value::Mapping(refs)) = root.get_mut(serde_yaml::Value::from("refs")) {
        let name_key = serde_yaml::Value::from(name);
        let removable = match (only_if_matches, refs.get(&name_key)) {
            (Some(expected), Some(v)) => v.as_str() == Some(expected),
            _ => only_if_matches.is_none(),
        };
        if removable {
            refs.remove(name_key);
        }
    }
    write_yaml_value(&path, &serde_yaml::Value::Mapping(root), "credentials")
}

// ---------- 编排层 ----------

pub fn sync_aggregate_entry(
    token: &str,
    models: &[crate::mode::stack::DesktopStackModel],
) -> Result<(), AppError> {
    let spec = DshProviderSpec {
        display_name: "CC Switch Aggregate".into(),
        api: "anthropic-messages".into(),
        base_url: "http://127.0.0.1:15721/claude-desktop".into(),
        api_key_env: AGGREGATE_TOKEN_ENV.into(),
        models: models
            .iter()
            .map(|model| DshModelEntry {
                id: model.id.clone(),
                name: model.label.clone(),
                ..DshModelEntry::metadata_none()
            })
            .collect(),
    };
    upsert_provider(AGGREGATE_ROUTE, &spec)?;
    upsert_credential_ref(&get_dsh_dir()?, AGGREGATE_TOKEN_ENV, token)
}

/// 供应商卡 settings_config → patch 条目 + 凭据 ref。
/// 卡形状：{displayName, api, baseUrl, apiKey, models:[{id,name}]}。
/// Exact endpoint/model contracts, audited against vendor docs and pi-ai 0.87.1.
/// A reasoning boolean alone deliberately offers no fabricated effort levels.
fn model_capability(
    base_url: &str,
    api: &str,
    id: &str,
    model: &serde_json::Map<String, Value>,
) -> Result<Value, AppError> {
    let url = base_url.trim_end_matches('/');
    let mut result = json!({});
    let tiers = json!({"low":"low", "high":"high", "max":"max"});
    if api == "openai-completions" {
        if matches!(
            url,
            "https://api.deepseek.com"
                | "https://api.deepseek.com/v1"
                | "https://opencode.ai/zen/go/v1"
        ) && matches!(id, "deepseek-flash" | "deepseek-v4-pro")
        {
            result = json!({"reasoningEfforts":{"off":null,"high":"high","max":"max"},"compat":{"thinkingFormat":"deepseek","supportsReasoningEffort":true,"maxTokensField":"max_tokens","requiresReasoningContentOnAssistantMessages":true}});
            if id == "deepseek-flash" {
                result["reasoningEfforts"]["low"] = json!("low");
            }
        } else if matches!(
            url,
            "https://open.bigmodel.cn/api/coding/paas/v4" | "https://open.bigmodel.cn/api/paas/v4"
        ) && matches!(id, "glm-5.3" | "glm-5.3-flash")
        {
            result = json!({"reasoningEfforts":tiers,"compat":{"thinkingFormat":"zai","supportsReasoningEffort":true,"maxTokensField":"max_tokens"}});
        } else if url == "https://opencode.ai/zen/go/v1"
            && matches!(id, "glm-5.3" | "glm-5.3-flash")
        {
            result = json!({"reasoningEfforts":tiers,"compat":{"supportsReasoningEffort":true,"maxTokensField":"max_tokens"}});
        }
    } else if api == "openai-responses"
        && matches!(
            url,
            "https://api.deepseek.com" | "https://api.deepseek.com/v1"
        )
        && matches!(id, "deepseek-flash" | "deepseek-v4-pro")
    {
        result = json!({"reasoningEfforts":{"off":"none","low":"low","high":"high","max":"max"}});
    } else if api == "anthropic-messages"
        && matches!(
            url,
            "https://api.kimi.com/coding" | "https://api.kimi.ai/coding"
        )
        && matches!(id, "k3" | "k3-256k" | "kimi-for-coding")
    {
        result = json!({"reasoningEfforts":tiers,"compat":{"forceAdaptiveThinking":true,"allowEmptySignature":true}});
    } else if api == "anthropic-messages"
        && url == "https://opencode.ai/zen/go/v1"
        && id == "claude-haiku-5-5"
    {
        result = json!({"reasoningEfforts":{"off":null,"low":"low","medium":"medium","high":"high","xhigh":"xhigh","max":"max"},"compat":{"forceAdaptiveThinking":true}});
    }
    if model.get("reasoning") == Some(&json!(false)) {
        result = json!({"reasoningEfforts":false});
    }
    if let Some(efforts) = model.get("reasoningEfforts") {
        if efforts != &json!(false) {
            let map = efforts.as_object().ok_or_else(|| {
                AppError::Config(format!(
                    "{id}: reasoningEfforts must be false or an effort mapping"
                ))
            })?;
            if !map
                .iter()
                .any(|(k, v)| k != "off" && v.as_str().is_some_and(|s| !s.is_empty()))
                || map.iter().any(|(k, v)| {
                    !["off", "minimal", "low", "medium", "high", "xhigh", "max"]
                        .contains(&k.as_str())
                        || !(v.as_str().is_some_and(|s| !s.is_empty())
                            || (k == "off" && v.is_null()))
                })
            {
                return Err(AppError::Config(format!(
                    "{id}: invalid reasoningEfforts mapping"
                )));
            }
        }
        result["reasoningEfforts"] = efforts.clone();
    }
    if let Some(compat) = model.get("compat") {
        let fields = compat
            .as_object()
            .ok_or_else(|| AppError::Config(format!("{id}: compat must be an object")))?;
        if result.get("compat").is_none() {
            result["compat"] = json!({});
        }
        for (key, value) in fields {
            validate_model_compat(api, key, value)?;
            result["compat"][key] = value.clone();
        }
    }
    Ok(result)
}

fn validate_model_compat(api: &str, key: &str, value: &Value) -> Result<(), AppError> {
    let completions = [
        "supportsStore",
        "supportsDeveloperRole",
        "supportsReasoningEffort",
        "supportsUsageInStreaming",
        "supportsFinishReason",
        "maxTokensField",
        "requiresToolResultName",
        "requiresAssistantAfterToolResult",
        "requiresThinkingAsText",
        "requiresReasoningContentOnAssistantMessages",
        "thinkingFormat",
        "chatTemplateKwargs",
        "chatTemplateArgs",
        "supportsThinkingTokenBudget",
        "thinkingTokenBudgetField",
        "vllmPriority",
        "supportsStrictMode",
        "cacheControlFormat",
        "supportsLongCacheRetention",
    ];
    let responses = [
        "supportsDeveloperRole",
        "supportsMaxOutputTokens",
        "supportsStrictMode",
        "supportsLongCacheRetention",
    ];
    let messages = [
        "supportsEagerToolInputStreaming",
        "supportsLongCacheRetention",
        "supportsCacheControlOnTools",
        "supportsTemperature",
        "forceAdaptiveThinking",
        "allowEmptySignature",
        "supportsStrictTools",
    ];
    let offered = match api {
        "openai-completions" => completions.contains(&key),
        "openai-responses" => responses.contains(&key),
        "anthropic-messages" => messages.contains(&key),
        _ => false,
    };
    let valid = match key {
        "thinkingFormat" => value.as_str().is_some_and(|s| {
            [
                "openai",
                "deepseek",
                "openrouter",
                "together",
                "baseten",
                "zai",
                "qwen",
                "chat-template",
                "qwen-chat-template",
                "string-thinking",
                "ant-ling",
            ]
            .contains(&s)
        }),
        "maxTokensField" => value
            .as_str()
            .is_some_and(|s| ["max_tokens", "max_completion_tokens"].contains(&s)),
        "thinkingTokenBudgetField" => value.as_str().is_some_and(|s| {
            [
                "thinking_token_budget",
                "thinking_budget",
                "thinking_budget_tokens",
            ]
            .contains(&s)
        }),
        "cacheControlFormat" => value.as_str() == Some("anthropic"),
        "vllmPriority" => value.is_i64(),
        "chatTemplateKwargs" | "chatTemplateArgs" => value.as_object().is_some_and(|map| {
            map.values().all(|v| {
                v.is_null()
                    || v.is_boolean()
                    || v.is_number()
                    || v.is_string()
                    || v.as_object().is_some_and(|m| {
                        m.get("$var").and_then(Value::as_str).is_some_and(|s| {
                            ["thinking.enabled", "thinking.effort", "thinking.budget"].contains(&s)
                        }) && m.get("omitWhenOff").is_none_or(Value::is_boolean)
                            && m.keys().all(|k| k == "$var" || k == "omitWhenOff")
                    })
            })
        }),
        _ => value.is_boolean(),
    };
    if !offered || !valid {
        return Err(AppError::Config(format!(
            "Invalid {api} model compat.{key}; no profiles were changed"
        )));
    }
    Ok(())
}

pub fn write_provider_card(
    route: &str,
    provider_name: &str,
    settings_config: &Value,
) -> Result<(), AppError> {
    // Mcode 同构卡：settingsConfig = { name, kind, enabled, api,
    //   options: { baseURL, apiKey, headers? },
    //   models: { "<id>": { name, reasoning, modalities, limit } } }
    // 写入 cordis.patch.yml 时转为 pi-ai 条目。
    let options = settings_config
        .get("options")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Config("DeepSeek Harness 卡缺少 options 字段".into()))?;

    let api = settings_config
        .get("api")
        .and_then(Value::as_str)
        .unwrap_or("openai-completions");
    match api {
        "anthropic-messages" | "openai-responses" | "openai-completions" => {}
        other => {
            return Err(AppError::Config(format!(
                "DeepSeek Harness api 不支持: {other}（可选 openai-completions / openai-responses / anthropic-messages）"
            )))
        }
    }

    let base_url = options
        .get("baseURL")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if base_url.is_empty() {
        return Err(AppError::Config(
            "DeepSeek Harness 供应商需要 options.baseURL".into(),
        ));
    }
    // Kimi Code's coding endpoint uses Messages, unlike the platform's /v1 API.
    let api = if matches!(
        base_url.trim_end_matches('/'),
        "https://api.kimi.com/coding" | "https://api.kimi.ai/coding"
    ) {
        "anthropic-messages"
    } else {
        api
    };

    let api_key = options
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    // models: Record<id, { name?, reasoning?, modalities?, limit? }> → pi-ai 数组
    let models_map = settings_config
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Config("DeepSeek Harness 卡缺少 models 字段".into()))?;
    if models_map.is_empty() {
        return Err(AppError::Config(
            "DeepSeek Harness 供应商需要至少一个模型".into(),
        ));
    }

    let env_name = credential_env_for_route(route);
    let mut grouped_models: std::collections::BTreeMap<&str, serde_yaml::Sequence> =
        std::collections::BTreeMap::new();
    for (model_id, model_val) in models_map {
        let Some(model) = model_val.as_object() else {
            continue;
        };
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::from("id"),
            serde_yaml::Value::from(model_id.as_str()),
        );
        if let Some(name) = model.get("name").and_then(Value::as_str) {
            entry.insert(
                serde_yaml::Value::from("name"),
                serde_yaml::Value::from(name),
            );
        }
        let model_api = if base_url.trim_end_matches('/') == "https://opencode.ai/zen/go/v1"
            && model_id.starts_with("claude-")
        {
            "anthropic-messages"
        } else {
            api
        };
        let capability = model_capability(&base_url, model_api, model_id, model)?;
        for (key, value) in capability.as_object().expect("capability object") {
            entry.insert(serde_yaml::Value::from(key.as_str()), json_to_yaml(value));
        }
        // modalities.input → input（pi-ai 扁平）
        if let Some(input) = model
            .get("modalities")
            .and_then(|m| m.get("input"))
            .and_then(Value::as_array)
        {
            let list: Vec<serde_yaml::Value> = input
                .iter()
                .filter_map(Value::as_str)
                .filter(|modality| matches!(*modality, "text" | "image"))
                .map(serde_yaml::Value::from)
                .collect();
            if !list.is_empty() {
                entry.insert(
                    serde_yaml::Value::from("input"),
                    serde_yaml::Value::Sequence(list),
                );
            }
        }
        // limit.context → contextWindow / limit.output → maxTokens
        if let Some(limit) = model.get("limit").and_then(Value::as_object) {
            if let Some(context) = limit.get("context").and_then(Value::as_i64) {
                entry.insert(
                    serde_yaml::Value::from("contextWindow"),
                    serde_yaml::Value::from(context),
                );
            }
            if let Some(output) = limit.get("output").and_then(Value::as_i64) {
                entry.insert(
                    serde_yaml::Value::from("maxTokens"),
                    serde_yaml::Value::from(output),
                );
            }
        }
        grouped_models
            .entry(model_api)
            .or_default()
            .push(serde_yaml::Value::Mapping(entry));
    }

    // 构造 pi-ai provider 条目
    let display_name = settings_config
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(provider_name);
    let mut provider = serde_yaml::Mapping::new();
    provider.insert(serde_yaml::Value::from("api"), serde_yaml::Value::from(api));
    provider.insert(
        serde_yaml::Value::from("baseURL"),
        serde_yaml::Value::from(base_url.as_str()),
    );
    provider.insert(
        serde_yaml::Value::from("apiKeyEnv"),
        serde_yaml::Value::from(env_name.as_str()),
    );
    provider.insert(
        serde_yaml::Value::from("displayName"),
        serde_yaml::Value::from(display_name),
    );
    if grouped_models.is_empty() {
        return Err(AppError::Config(
            "DeepSeek Harness 供应商没有有效模型".into(),
        ));
    }
    // Existing primary protocol is a durable route binding, independent of selections.
    let mut bound_api: Option<String> = None;
    for path in get_all_patch_paths()? {
        if let Some(existing) = read_providers_from_file(&path)?.get(route) {
            if existing["apiKeyEnv"].as_str() == Some(env_name.as_str()) {
                if let Some(value) = existing["api"].as_str() {
                    if bound_api.as_deref().is_some_and(|bound| bound != value) {
                        return Err(AppError::Config(format!("DeepSeek Harness {route} has conflicting protocol bindings across profiles")));
                    }
                    bound_api = Some(value.to_string());
                }
            }
        }
    }
    let primary_api = bound_api.as_deref().unwrap_or(api);
    // An empty primary catalog keeps the identity discoverable without adding models.
    grouped_models.entry(primary_api).or_default();

    // 写 patch（全部活跃 profile）
    let mut pending_profiles = Vec::new();
    for path in get_all_patch_paths()? {
        let mut entries = read_patch_entries(&path)?;
        let providers = providers_mut(&mut entries);
        if let Some(existing) = providers.get(serde_yaml::Value::from(route)) {
            if existing
                .get("apiKeyEnv")
                .and_then(serde_yaml::Value::as_str)
                != Some(env_name.as_str())
            {
                return Err(AppError::Config(format!(
                    "DeepSeek Harness 通道 {route} 与非托管供应商冲突"
                )));
            }
        }
        for model_api in grouped_models
            .keys()
            .filter(|model_api| **model_api != primary_api)
        {
            let target = serde_yaml::Value::from(format!("{route}--{model_api}"));
            if let Some(existing) = providers.get(&target) {
                if existing
                    .get("apiKeyEnv")
                    .and_then(serde_yaml::Value::as_str)
                    != Some(env_name.as_str())
                {
                    return Err(AppError::Config(format!(
                        "DeepSeek Harness 派生通道 {route}--{model_api} 与已有供应商冲突"
                    )));
                }
            }
        }
        remove_protocol_children(providers, route);
        for (model_api, models) in &grouped_models {
            let target = if *model_api == primary_api {
                route.to_string()
            } else {
                format!("{route}--{model_api}")
            };
            let mut exported = provider.clone();
            exported.insert(
                serde_yaml::Value::from("api"),
                serde_yaml::Value::from(*model_api),
            );
            exported.insert(
                serde_yaml::Value::from("models"),
                serde_yaml::Value::Sequence(models.clone()),
            );
            if *model_api == "anthropic-messages"
                && base_url.trim_end_matches('/') == "https://opencode.ai/zen/go/v1"
            {
                exported.insert(
                    serde_yaml::Value::from("baseURL"),
                    serde_yaml::Value::from("https://opencode.ai/zen/go"),
                );
            }
            if target != route {
                exported.insert(
                    serde_yaml::Value::from("displayName"),
                    serde_yaml::Value::from(format!("{display_name} · Messages")),
                );
            }
            providers.insert(
                serde_yaml::Value::from(target),
                serde_yaml::Value::Mapping(exported),
            );
        }
        pending_profiles.push((path, serde_yaml::Value::Sequence(entries)));
    }
    // Validate every profile before applying any deterministic configuration change.
    for (path, entries) in pending_profiles {
        write_yaml_value(&path, &entries, "cordis.patch")?;
    }

    if !api_key.is_empty() {
        upsert_credential_ref(&get_dsh_dir()?, &env_name, &api_key)?;
    }
    Ok(())
}

pub fn remove_provider_card(route: &str) -> Result<(), AppError> {
    remove_provider(route)?;
    remove_credential_ref(&get_dsh_dir()?, &credential_env_for_route(route), None)
}

/// Non-secret ownership index consumed by terminal clients, separate from DSH's schema.
fn ownership_path() -> Result<PathBuf, AppError> {
    #[cfg(test)]
    if let Some(path) = TEST_DSH_PATCH_PATH.lock().expect("test path").clone() {
        return Ok(path.with_file_name("cc-switch-managed.json"));
    }
    Ok(get_dsh_dir()?.join("cc-switch-managed.json"))
}

fn read_ownership() -> Result<Value, AppError> {
    let path = ownership_path()?;
    if !path.exists() {
        return Ok(json!({"version":1,"providers":{}}));
    }
    let value: Value = crate::config::read_json_file(&path)?;
    if value["version"] != json!(1) || !value["providers"].is_object() {
        return Err(AppError::Config(
            "Invalid CC-Switch DSH ownership index; repair it before syncing".into(),
        ));
    }
    Ok(value)
}

/// All lifecycle operations bind to the immutable DB ID, never the display name.
pub fn write_managed_provider_card(id: &str, name: &str, config: &Value) -> Result<(), AppError> {
    let route = sanitize_route(id);
    let mut ownership = read_ownership()?;
    write_provider_card(&route, name, config)?;
    let mut providers = indexmap::IndexMap::new();
    for path in get_all_patch_paths()? {
        providers.extend(read_providers_from_file(&path)?);
    }
    let env = credential_env_for_route(&route);
    let routes: Vec<_> = providers
        .iter()
        .filter(|(key, value)| {
            (key.as_str() == route || key.starts_with(&format!("{route}--")))
                && value["apiKeyEnv"].as_str() == Some(env.as_str())
        })
        .map(|(key, _)| key.clone())
        .collect();
    ownership["providers"][id] = json!({"route":route,"routes":routes,"displayName":name});
    crate::config::write_json_file(&ownership_path()?, &ownership)
}

pub fn remove_managed_provider_card(id: &str) -> Result<(), AppError> {
    let mut ownership = read_ownership()?;
    let route = sanitize_route(id);
    let env = credential_env_for_route(&route);
    // Refuse foreign replacements before changing any profile or credential.
    let paths = get_all_patch_paths()?;
    for path in &paths {
        for (key, value) in read_providers_from_file(path)? {
            if (key == route || key.starts_with(&format!("{route}--")))
                && value["apiKeyEnv"].as_str() != Some(env.as_str())
            {
                return Err(AppError::Config(format!(
                    "DeepSeek Harness {key} is not owned by this card; deletion refused"
                )));
            }
        }
    }
    remove_provider(&route)?;
    let referenced = paths
        .iter()
        .map(|path| read_providers_from_file(path))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|providers| {
            providers
                .values()
                .any(|value| value["apiKeyEnv"].as_str() == Some(env.as_str()))
        });
    if !referenced {
        remove_credential_ref(&get_dsh_dir()?, &env, None)?;
    }
    ownership["providers"]
        .as_object_mut()
        .expect("validated")
        .remove(id);
    crate::config::write_json_file(&ownership_path()?, &ownership)
}

pub fn managed_provider_exists(id: &str) -> Result<bool, AppError> {
    let route = sanitize_route(id);
    let env = credential_env_for_route(&route);
    for path in get_all_patch_paths()? {
        if read_providers_from_file(&path)?
            .get(&route)
            .is_some_and(|p| p["apiKeyEnv"].as_str() == Some(env.as_str()))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// route → 凭据 env 名：小写连字符转大写下划线，前缀 CC_SWITCH_DSH_。
pub fn credential_env_for_route(route: &str) -> String {
    format!("CC_SWITCH_DSH_{}", route.to_uppercase().replace('-', "_"))
}

/// 供应商 id → 合法 route 名（pi-ai 要求小写连字符标识符）。
pub fn sanitize_route(id: &str) -> String {
    let cleaned: String = id
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        format!("provider-{}", &id.chars().take(8).collect::<String>())
    } else {
        trimmed
    }
}

impl DshModelEntry {
    /// 聚合条目不带容量元数据（路由表没有这些信息）。
    fn metadata_none() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            reasoning: None,
            modalities: None,
            limit: None,
        }
    }
}

/// Refresh the existing `cc-switch` DSH route from the Desktop Stack models
/// `route_profile` just published. A missing route stays missing, so a Desktop
/// proxy user who never opted into DSH does not get a `~/.dsh` write.
pub fn sync_dsh_aggregate_now(
    db: &crate::database::Database,
    models: &[crate::mode::stack::DesktopStackModel],
) -> Result<(), AppError> {
    if !aggregate_entry_exists()? {
        return Ok(());
    }
    let token = crate::claude_desktop_config::get_or_create_gateway_token(db)?;
    sync_aggregate_entry(&token, models)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    struct PatchGuard(PathBuf);
    impl PatchGuard {
        fn new(dir: &TempDir) -> Self {
            let path = dir.path().join("cordis.patch.yml");
            *TEST_DSH_PATCH_PATH.lock().expect("lock test patch path") = Some(path.clone());
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for PatchGuard {
        fn drop(&mut self) {
            *TEST_DSH_PATCH_PATH.lock().expect("lock test patch path") = None;
            TEST_DSH_EXTRA_PATCH_PATHS
                .lock()
                .expect("lock extra test paths")
                .clear();
        }
    }

    fn spec(route_env: &str) -> DshProviderSpec {
        DshProviderSpec {
            display_name: "Foo".into(),
            api: "openai-completions".into(),
            base_url: "https://x.example".into(),
            api_key_env: route_env.into(),
            models: vec![DshModelEntry {
                id: "m1".into(),
                name: "M One".into(),
                ..DshModelEntry::metadata_none()
            }],
        }
    }

    #[test]
    #[serial]
    fn upsert_then_read_round_trip() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        upsert_provider("foo", &spec("CC_SWITCH_DSH_FOO")).unwrap();
        let providers = read_providers().unwrap();
        let foo = providers
            .get("foo")
            .expect("foo route")
            .as_object()
            .unwrap();
        assert_eq!(foo["api"], json!("openai-completions"));
        assert_eq!(foo["baseURL"], json!("https://x.example"));
        assert_eq!(foo["apiKeyEnv"], json!("CC_SWITCH_DSH_FOO"));
        assert_eq!(foo["displayName"], json!("Foo"));
        assert_eq!(foo["models"][0]["id"], json!("m1"));
        assert!(guard.path().exists());
    }

    #[test]
    #[serial]
    fn upsert_preserves_other_entries_and_routes() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        std::fs::write(
            guard.path(),
            "- id: ui-chat\n  name: \"@deepseek-ai/dsh-client-ui-chat\"\n  config:\n    transcriptView: standard\n- id: llm-pi-ai\n  name: \"@deepseek-ai/dsh-llm-pi-ai\"\n  config:\n    providers:\n      keep:\n        api: openai-responses\n",
        )
        .unwrap();
        upsert_provider("foo", &spec("CC_SWITCH_DSH_FOO")).unwrap();
        let providers = read_providers().unwrap();
        assert!(providers.contains_key("keep"));
        assert_eq!(providers["keep"]["api"], json!("openai-responses"));
        let text = std::fs::read_to_string(guard.path()).unwrap();
        assert!(text.contains("ui-chat"));
        assert!(text.contains("transcriptView"));
    }

    #[test]
    #[serial]
    fn remove_preserves_other_entries_and_routes() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        std::fs::write(
            guard.path(),
            "- id: ui-theme\n  name: \"@deepseek-ai/dsh-client-ui-theme\"\n  config:\n    preference: system\n- id: llm-pi-ai\n  config:\n    providers:\n      keep:\n        api: x\n      foo:\n        api: y\n",
        )
        .unwrap();
        remove_provider("foo").unwrap();
        let providers = read_providers().unwrap();
        assert!(providers.contains_key("keep"));
        assert!(!providers.contains_key("foo"));
        let text = std::fs::read_to_string(guard.path()).unwrap();
        assert!(text.contains("ui-theme"));
    }

    #[test]
    #[serial]
    fn remove_last_provider_prunes_llm_entry() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        upsert_provider("foo", &spec("E")).unwrap();
        remove_provider("foo").unwrap();
        let text = std::fs::read_to_string(guard.path()).unwrap();
        assert!(!text.contains("llm-pi-ai"));
        upsert_provider("bar", &spec("E2")).unwrap();
        upsert_provider("baz", &spec("E3")).unwrap();
        remove_provider("bar").unwrap();
        assert!(std::fs::read_to_string(guard.path())
            .unwrap()
            .contains("llm-pi-ai"));
    }

    #[test]
    fn credential_refs_upsert_and_remove_preserving_records() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(CREDENTIALS_FILE),
            "version: 1\nrefs:\n  OTHER: k1\nrecords:\n  - kind: oauth\n",
        )
        .unwrap();
        upsert_credential_ref(dir.path(), "CC_SWITCH_DSH_FOO", "sk-1").unwrap();
        upsert_credential_ref(dir.path(), "CC_SWITCH_DSH_FOO", "sk-2").unwrap();
        remove_credential_ref(dir.path(), "CC_SWITCH_DSH_FOO", Some("wrong")).unwrap();
        let text = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(text.contains("CC_SWITCH_DSH_FOO: sk-2"));
        remove_credential_ref(dir.path(), "CC_SWITCH_DSH_FOO", Some("sk-2")).unwrap();
        let text = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(text.contains("OTHER: k1"));
        assert!(text.contains("records"));
        assert!(!text.contains("CC_SWITCH_DSH_FOO"));
    }

    fn stack_member(
        key: &str,
        name: &str,
        upstream: &str,
        label: &str,
    ) -> crate::mode::stack::Member {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "claude-sonnet-5".into(),
            crate::provider::ClaudeDesktopModelRoute {
                model: upstream.into(),
                label_override: Some(label.into()),
                supports_1m: Some(false),
            },
        );
        let mut provider =
            crate::provider::Provider::with_id(key.into(), name.into(), json!({}), None);
        provider.meta = Some(crate::provider::ProviderMeta {
            claude_desktop_model_routes: routes,
            ..Default::default()
        });
        crate::mode::stack::Member {
            provider,
            key: key.into(),
            route: false,
            model_ids: Vec::new(),
        }
    }

    #[test]
    #[serial]
    fn dsh_aggregate_model_list_follows_desktop_stack_publication() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let db = crate::database::Database::memory().unwrap();
        let stale = vec![crate::mode::stack::DesktopStackModel {
            id: "claude-fable-1".into(),
            upstream: "old-model".into(),
            label: "Old slot".into(),
            supports_1m: false,
        }];
        sync_aggregate_entry("seed-token", &stale).unwrap();

        let published = crate::mode::stack::desktop_published(&[stack_member(
            "zhipu", "Zhipu", "glm-5.3", "GLM 5.3",
        )]);
        assert_eq!(published.len(), 1);
        assert!(published[0].id.starts_with("claude-fable-ccs-"));
        assert_eq!(published[0].upstream, "glm-5.3");
        assert_eq!(published[0].label, "Zhipu / GLM 5.3");
        assert_ne!(published[0].id, published[0].upstream);

        sync_dsh_aggregate_now(&db, &published).unwrap();

        let providers = read_providers().unwrap();
        let ccsw = providers.get(AGGREGATE_ROUTE).unwrap().as_object().unwrap();
        assert_eq!(ccsw["api"], json!("anthropic-messages"));
        assert_eq!(
            ccsw["baseURL"],
            json!("http://127.0.0.1:15721/claude-desktop")
        );
        assert_eq!(ccsw["apiKeyEnv"], json!(AGGREGATE_TOKEN_ENV));
        assert_eq!(ccsw["displayName"], json!("CC Switch Aggregate"));
        assert_eq!(ccsw["models"].as_array().unwrap().len(), 1);
        assert_eq!(ccsw["models"][0]["id"], json!(published[0].id));
        assert_eq!(ccsw["models"][0]["name"], json!(published[0].label));
        let text = std::fs::read_to_string(guard_patch(&dir)).unwrap();
        assert!(!text.contains("claude-fable-1"));
        assert!(!text.contains("old-model"));
        let creds = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(creds.contains(AGGREGATE_TOKEN_ENV));
    }

    fn guard_patch(dir: &TempDir) -> PathBuf {
        dir.path().join("cordis.patch.yml")
    }

    #[test]
    #[serial]
    fn write_card_validates_and_writes_ref() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        // write_provider_card 的凭据写真实家目录——测试仅验证 patch 侧与校验；
        // 直接构造等价 spec 验证文件形状：
        let spec = DshProviderSpec {
            display_name: "MyGW".into(),
            api: "openai-responses".into(),
            base_url: "https://gw.example/v1".into(),
            api_key_env: credential_env_for_route("my-gw"),
            models: vec![DshModelEntry {
                id: "g1".into(),
                name: "G1".into(),
                ..DshModelEntry::metadata_none()
            }],
        };
        upsert_provider("my-gw", &spec).unwrap();
        let providers = read_providers().unwrap();
        assert_eq!(
            providers["my-gw"]["apiKeyEnv"],
            json!("CC_SWITCH_DSH_MY_GW")
        );
        // 非法 api / 空字段拒绝（write_provider_card 内联校验）：
        let bad = json!({"api": "bogus", "baseUrl": "x", "models": [{"id": "m"}]});
        assert!(write_provider_card("bad", "B", &bad).is_err());
        let no_url = json!({"api": "openai-completions", "baseUrl": " ", "models": [{"id": "m"}]});
        assert!(write_provider_card("n1", "N", &no_url).is_err());
        let no_models = json!({"api": "openai-completions", "baseUrl": "https://x", "models": []});
        assert!(write_provider_card("n2", "N", &no_models).is_err());
    }

    #[test]
    #[serial]
    fn write_card_preserves_explicit_reasoning_contract() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let cfg = json!({
            "api": "openai-completions",
            "options": {"baseURL": "https://controlled.invalid/v1", "apiKey": ""},
            "models": {
                "a": {"reasoning": true, "reasoningEfforts": {"off": null, "high": "custom-high"}, "compat": {"thinkingFormat": "deepseek"}},
                "b": {"reasoning": true},
                "c": {"reasoning": false}
            }
        });
        write_provider_card("controlled", "Controlled", &cfg).unwrap();
        let providers = read_providers().unwrap();
        let models = providers["controlled"]["models"].as_array().unwrap();
        assert_eq!(
            models[0]["reasoningEfforts"],
            cfg["models"]["a"]["reasoningEfforts"]
        );
        assert_eq!(models[0]["compat"], cfg["models"]["a"]["compat"]);
        assert!(
            models[1].get("reasoningEfforts").is_none(),
            "a boolean is not a list of supported levels"
        );
        assert_eq!(models[2]["reasoningEfforts"], json!(false));
    }

    #[test]
    #[serial]
    fn write_card_exports_verified_endpoint_capabilities() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let cfg = json!({"api": "openai-completions", "options": {"baseURL": "https://open.bigmodel.cn/api/coding/paas/v4", "apiKey": ""}, "models": {"glm-5.3": {"reasoning": true}}});
        write_provider_card("custom-id", "Renamable label", &cfg).unwrap();
        let providers = read_providers().unwrap();
        assert_eq!(
            providers["custom-id"]["models"][0]["reasoningEfforts"],
            json!({"low":"low", "high":"high", "max":"max"})
        );
        assert_eq!(
            providers["custom-id"]["models"][0]["compat"]["thinkingFormat"],
            json!("zai")
        );
    }

    #[test]
    fn responses_and_foreign_endpoint_capabilities_are_protocol_specific() {
        let model = json!({"reasoning":true});
        let resolved = model_capability(
            "https://api.deepseek.com",
            "openai-responses",
            "deepseek-flash",
            model.as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(resolved["reasoningEfforts"]["off"], json!("none"));
        assert!(
            resolved.get("compat").is_none(),
            "Responses must not receive Chat Completions switches"
        );
        assert_eq!(
            model_capability(
                "https://foreign.invalid",
                "openai-completions",
                "glm-5.3",
                model.as_object().unwrap()
            )
            .unwrap(),
            json!({})
        );
    }

    #[test]
    #[serial]
    fn managed_delete_refuses_foreign_route_before_any_write() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        let cfg = json!({"api":"openai-completions","options":{"baseURL":"https://controlled.invalid","apiKey":""},"models":{"a":{}}});
        write_managed_provider_card("stable-id", "Name", &cfg).unwrap();
        let other = dir.path().join("other.yml");
        std::fs::copy(guard.path(), &other).unwrap();
        let mut entries = read_patch_entries(&other).unwrap();
        providers_mut(&mut entries)
            .get_mut(serde_yaml::Value::from("stable-id"))
            .unwrap()["apiKeyEnv"] = serde_yaml::Value::from("FOREIGN_REF");
        write_yaml_value(&other, &serde_yaml::Value::Sequence(entries), "test").unwrap();
        TEST_DSH_EXTRA_PATCH_PATHS
            .lock()
            .unwrap()
            .push(other.clone());
        let before = std::fs::read(guard.path()).unwrap();
        assert!(remove_managed_provider_card("stable-id").is_err());
        assert_eq!(std::fs::read(guard.path()).unwrap(), before);
        assert!(read_ownership().unwrap()["providers"]
            .get("stable-id")
            .is_some());
    }

    #[test]
    #[serial]
    fn model_selection_preserves_protocol_identity() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let mut cfg = json!({"api":"openai-completions","options":{"baseURL":"https://opencode.ai/zen/go/v1","apiKey":""},"models":{"glm-5.3":{},"claude-haiku-5-5":{}}});
        write_provider_card("stable", "Mixed", &cfg).unwrap();
        cfg["models"].as_object_mut().unwrap().remove("glm-5.3");
        write_provider_card("stable", "Claude only", &cfg).unwrap();
        assert_eq!(
            read_providers().unwrap()["stable--anthropic-messages"]["models"][0]["id"],
            json!("claude-haiku-5-5")
        );
        cfg["models"]["glm-5.3"] = json!({});
        write_provider_card("stable", "Mixed again", &cfg).unwrap();
        assert_eq!(
            read_providers().unwrap()["stable--anthropic-messages"]["api"],
            json!("anthropic-messages")
        );
    }

    #[test]
    #[serial]
    fn invalid_compat_is_rejected_before_writing() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        let mut cfg = json!({"api":"anthropic-messages","options":{"baseURL":"https://controlled.invalid","apiKey":""},"models":{"a":{}}});
        write_provider_card("stable", "Name", &cfg).unwrap();
        let before = std::fs::read(guard.path()).unwrap();
        for invalid in [
            json!({"thinkingFormat":"deepseek"}),
            json!({"forceAdaptiveThinking":"yes"}),
            json!({"unknownField":true}),
        ] {
            cfg["models"]["a"]["compat"] = invalid;
            assert!(write_provider_card("stable", "Name", &cfg).is_err());
            assert_eq!(std::fs::read(guard.path()).unwrap(), before);
        }
    }

    #[test]
    #[serial]
    fn managed_identity_survives_rename_and_preserves_history() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let cfg = json!({"api":"openai-completions","options":{"baseURL":"https://controlled.invalid","apiKey":""},"models":{"a":{}}});
        write_provider_card("old-label", "Old label", &cfg).unwrap();
        write_managed_provider_card("stable-id", "Old label", &cfg).unwrap();
        write_managed_provider_card("stable-id", "Renamed", &cfg).unwrap();
        let providers = read_providers().unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers["stable-id"]["displayName"], json!("Renamed"));
        assert_eq!(
            read_ownership().unwrap()["providers"]["stable-id"]["routes"],
            json!(["stable-id"])
        );
        // Test file-layer deletion without touching the real credential store.
        remove_provider("stable-id").unwrap();
        assert!(read_providers().unwrap().contains_key("old-label"));
        let bad = json!({"api":"openai-completions","options":{"baseURL":"https://controlled.invalid"},"models":{"a":{"reasoningEfforts":{"off":null}}}});
        assert!(write_managed_provider_card("bad", "Invalid", &bad).is_err());
        assert!(read_ownership().unwrap()["providers"].get("bad").is_none());
    }

    #[test]
    #[serial]
    fn write_card_preserves_models_and_supported_inputs() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let cfg = json!({
            "api": "openai-completions",
            "options": { "baseURL": "https://controlled.invalid/v1", "apiKey": "" },
            "models": {
                "glm-5.3": { "name": "GLM 5.3", "modalities": { "input": ["text", "image", "video"] } },
                "glm-5.3-flash": { "name": "GLM Flash", "modalities": { "input": ["text"] } }
            }
        });
        write_provider_card("controlled", "Controlled", &cfg).unwrap();
        let providers = read_providers().unwrap();
        let models = providers["controlled"]["models"].as_array().unwrap();
        assert_eq!(
            models.len(),
            2,
            "export must retain every explicitly selected model"
        );
        assert_eq!(models[0]["id"], json!("glm-5.3"));
        assert_eq!(models[1]["id"], json!("glm-5.3-flash"));
        assert_eq!(models[0]["input"], json!(["text", "image"]));
    }

    #[test]
    #[serial]
    fn write_card_splits_opencode_protocols_and_removes_stale_children() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let mut cfg = json!({
            "api": "openai-completions",
            "options": { "baseURL": "https://opencode.ai/zen/go/v1", "apiKey": "" },
            "models": {
                "claude-haiku-5-5": { "name": "Claude Haiku" },
                "glm-5.3-flash": { "name": "GLM Flash" }
            }
        });
        write_provider_card("controlled", "OpenCode Go", &cfg).unwrap();
        let providers = read_providers().unwrap();
        assert_eq!(
            providers["controlled"]["models"][0]["id"],
            json!("glm-5.3-flash")
        );
        let child = &providers["controlled--anthropic-messages"];
        assert_eq!(child["api"], json!("anthropic-messages"));
        assert_eq!(child["models"][0]["id"], json!("claude-haiku-5-5"));
        assert_eq!(child["apiKeyEnv"], providers["controlled"]["apiKeyEnv"]);
        cfg["models"]
            .as_object_mut()
            .unwrap()
            .remove("claude-haiku-5-5");
        write_provider_card("controlled", "OpenCode Go", &cfg).unwrap();
        assert!(!read_providers()
            .unwrap()
            .contains_key("controlled--anthropic-messages"));
        upsert_provider("controlled--anthropic-messages", &spec("FOREIGN_REF")).unwrap();
        cfg["models"]["claude-haiku-5-5"] = json!({ "name": "Claude Haiku" });
        assert!(write_provider_card("controlled", "OpenCode Go", &cfg).is_err());
        assert_eq!(
            read_providers().unwrap()["controlled--anthropic-messages"]["apiKeyEnv"],
            json!("FOREIGN_REF")
        );
    }

    #[test]
    #[serial]
    fn write_card_preflights_all_profiles_before_mutation() {
        let dir = TempDir::new().unwrap();
        let guard = PatchGuard::new(&dir);
        upsert_provider("keep", &spec("KEEP_REF")).unwrap();
        let second = dir.path().join("second.patch.yml");
        upsert_provider_in_file(
            &second,
            "controlled--anthropic-messages",
            &spec("FOREIGN_REF"),
        )
        .unwrap();
        TEST_DSH_EXTRA_PATCH_PATHS
            .lock()
            .unwrap()
            .push(second.clone());
        let first_before = std::fs::read(guard.path()).unwrap();
        let second_before = std::fs::read(&second).unwrap();
        let cfg = json!({
            "api": "openai-completions",
            "options": { "baseURL": "https://opencode.ai/zen/go/v1", "apiKey": "" },
            "models": { "claude-haiku-5-5": { "name": "Claude" }, "glm-5.3-flash": { "name": "GLM" } }
        });
        assert!(write_provider_card("controlled", "OpenCode Go", &cfg).is_err());
        assert_eq!(
            std::fs::read(guard.path()).unwrap(),
            first_before,
            "later-profile refusal must not modify the first profile"
        );
        assert_eq!(std::fs::read(&second).unwrap(), second_before);
    }

    #[test]
    fn sanitize_route_normalizes_ids() {
        assert_eq!(sanitize_route("My Provider_2"), "my-provider-2");
        assert_eq!(sanitize_route("工具---x"), "x");
        assert!(sanitize_route("---").starts_with("provider-"));
    }
}
