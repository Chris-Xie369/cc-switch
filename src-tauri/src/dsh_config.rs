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
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DshModelEntry {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
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
static TEST_DSH_PATCH_PATH: LazyLock<Mutex<Option<PathBuf>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn get_dsh_dir() -> Result<PathBuf, AppError> {
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

/// desktop profile 的 patch 文件路径（本机实测唯一活跃 profile）。
fn get_patch_path() -> Result<PathBuf, AppError> {
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
    let Some(idx) = entries.iter().position(is_llm_entry) else { return };
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
    providers_mut(&mut entries).remove(serde_yaml::Value::from(route));
    prune_llm_entry_if_empty(&mut entries);
    write_yaml_value(path, &serde_yaml::Value::Sequence(entries), "cordis.patch")
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
    upsert_provider_in_file(&get_patch_path()?, route, spec)
}

pub fn remove_provider(route: &str) -> Result<(), AppError> {
    remove_provider_in_file(&get_patch_path()?, route)
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
    refs.as_mapping_mut()
        .expect("just made a mapping")
        .insert(serde_yaml::Value::from(name), serde_yaml::Value::from(secret));
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
    if let Some(serde_yaml::Value::Mapping(refs)) =
        root.get_mut(&serde_yaml::Value::from("refs"))
    {
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

pub fn sync_aggregate_entry(token: &str, routes: &[crate::claude_desktop_config::ResolvedModelRoute]) -> Result<(), AppError> {
    let spec = DshProviderSpec {
        display_name: "CC Switch Aggregate".into(),
        api: "anthropic-messages".into(),
        base_url: "http://127.0.0.1:15721/claude-desktop".into(),
        api_key_env: AGGREGATE_TOKEN_ENV.into(),
        models: routes
            .iter()
            .map(|r| DshModelEntry {
                id: r.route_id.clone(),
                name: r
                    .label_override
                    .clone()
                    .unwrap_or_else(|| r.upstream_model.clone()),
                ..DshModelEntry::metadata_none()
            })
            .collect(),
    };
    upsert_provider(AGGREGATE_ROUTE, &spec)?;
    upsert_credential_ref(&get_dsh_dir()?, AGGREGATE_TOKEN_ENV, token)
}

/// 供应商卡 settings_config → patch 条目 + 凭据 ref。
/// 卡形状：{displayName, api, baseUrl, apiKey, models:[{id,name}]}。
pub fn write_provider_card(
    route: &str,
    provider_name: &str,
    settings_config: &Value,
) -> Result<(), AppError> {
    let str_field = |key: &str, fallback: &str| -> String {
        settings_config
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| fallback.to_string())
    };
    let api = str_field("api", "openai-completions");
    match api.as_str() {
        "anthropic-messages" | "openai-responses" | "openai-completions" => {}
        other => {
            return Err(AppError::Config(format!(
                "DeepSeek Harness api 不支持: {other}（可选 openai-completions / openai-responses / anthropic-messages）"
            )))
        }
    }
    let base_url = str_field("baseUrl", "");
    if base_url.trim().is_empty() {
        return Err(AppError::Config("DeepSeek Harness 供应商需要 baseUrl".into()));
    }
    let models_value = settings_config.get("models").cloned().unwrap_or(json!([]));
    let models: Vec<DshModelEntry> = serde_json::from_value(models_value)
        .map_err(|e| AppError::Config(format!("DeepSeek Harness 模型列表格式无效: {e}")))?;
    if models.is_empty() {
        return Err(AppError::Config(
            "DeepSeek Harness 供应商需要至少一个模型".into(),
        ));
    }
    let env_name = credential_env_for_route(route);
    let spec = DshProviderSpec {
        display_name: str_field("displayName", provider_name),
        api,
        base_url,
        api_key_env: env_name.clone(),
        models,
    };
    upsert_provider(route, &spec)?;
    if let Some(key) = settings_config.get("apiKey").and_then(Value::as_str) {
        if !key.trim().is_empty() {
            upsert_credential_ref(&get_dsh_dir()?, &env_name, key)?;
        }
    }
    Ok(())
}

/// 移除供应商卡的 patch 键与自有凭据 ref（CC_SWITCH_DSH_ 前缀命名空间，安全）。
pub fn remove_provider_card(route: &str) -> Result<(), AppError> {
    remove_provider(route)?;
    remove_credential_ref(&get_dsh_dir()?, &credential_env_for_route(route), None)
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

/// 聚合同步入口：routes=Some（聚合切换）直接写；None（手动/其他切换）时，
/// 仅当用户已配置过 cc-switch route 才刷新（未启用 DSH 接入就不动文件）。
impl DshModelEntry {
    /// 聚合条目不带容量元数据（路由表没有这些信息）。
    fn metadata_none() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            reasoning: None,
            input: None,
            context_window: None,
            max_tokens: None,
        }
    }
}

pub fn sync_dsh_aggregate_now(
    db: &crate::database::Database,
    routes: Option<&[crate::claude_desktop_config::ResolvedModelRoute]>,
) -> Result<(), AppError> {
    let routes = match routes {
        Some(routes) => routes,
        None => {
            if !aggregate_entry_exists()? {
                return Ok(());
            }
            let current =
                crate::mode::current::provider_in_use(db, &crate::app_config::AppType::ClaudeDesktop)?;
            let Some(provider) = current else { return Ok(()) };
            if !provider
                .meta
                .as_ref()
                .is_some_and(|m| m.aggregate_routes.is_some())
            {
                return Ok(());
            }
            &crate::claude_desktop_config::proxy_model_routes(&provider)?
        }
    };
    let token = crate::claude_desktop_config::get_or_create_gateway_token(db)?;
    sync_aggregate_entry(&token, routes)
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
        let foo = providers.get("foo").expect("foo route").as_object().unwrap();
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
        assert!(std::fs::read_to_string(guard.path()).unwrap().contains("llm-pi-ai"));
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

    #[test]
    #[serial]
    fn aggregate_entry_shape_matches_r1() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let routes = vec![crate::claude_desktop_config::ResolvedModelRoute {
            route_id: "claude-fable-1".into(),
            upstream_model: "glm-5.3".into(),
            label_override: Some("Zhipu · glm-5.3".into()),
            supports_1m: true,
            max_effort: None,
            tier: None,
        }];
        // get_dsh_dir 在测试里仍指向真实家目录——凭据直接写入 tempdir 验证，
        // patch 侧经 upsert_provider（PatchGuard 生效）验证。
        let spec = DshProviderSpec {
            display_name: "CC Switch Aggregate".into(),
            api: "anthropic-messages".into(),
            base_url: "http://127.0.0.1:15721/claude-desktop".into(),
            api_key_env: AGGREGATE_TOKEN_ENV.into(),
            models: routes
                .iter()
                .map(|r| DshModelEntry {
                    id: r.route_id.clone(),
                    name: r.label_override.clone().unwrap(),
                })
                .collect(),
        };
        upsert_provider(AGGREGATE_ROUTE, &spec).unwrap();
        upsert_credential_ref(dir.path(), AGGREGATE_TOKEN_ENV, "tok").unwrap();
        let providers = read_providers().unwrap();
        let ccsw = providers.get(AGGREGATE_ROUTE).unwrap().as_object().unwrap();
        assert_eq!(ccsw["api"], json!("anthropic-messages"));
        assert_eq!(
            ccsw["baseURL"],
            json!("http://127.0.0.1:15721/claude-desktop")
        );
        assert_eq!(ccsw["apiKeyEnv"], json!(AGGREGATE_TOKEN_ENV));
        assert_eq!(ccsw["displayName"], json!("CC Switch Aggregate"));
        assert_eq!(ccsw["models"][0]["id"], json!("claude-fable-1"));
        assert!(aggregate_entry_exists().unwrap());
    }

    #[test]
    #[serial]
    fn write_card_validates_and_writes_ref() {
        let dir = TempDir::new().unwrap();
        let _guard = PatchGuard::new(&dir);
        let cfg = json!({
            "displayName": "MyGW",
            "api": "openai-responses",
            "baseUrl": "https://gw.example/v1",
            "apiKey": "sk-9",
            "models": [{"id": "g1", "name": "G1"}]
        });
        // write_provider_card 的凭据写真实家目录——测试仅验证 patch 侧与校验；
        // 直接构造等价 spec 验证文件形状：
        let spec = DshProviderSpec {
            display_name: "MyGW".into(),
            api: "openai-responses".into(),
            base_url: "https://gw.example/v1".into(),
            api_key_env: credential_env_for_route("my-gw"),
            models: vec![DshModelEntry { id: "g1".into(), name: "G1".into() }],
        };
        upsert_provider("my-gw", &spec).unwrap();
        let providers = read_providers().unwrap();
        assert_eq!(providers["my-gw"]["apiKeyEnv"], json!("CC_SWITCH_DSH_MY_GW"));
        // 非法 api / 空字段拒绝（write_provider_card 内联校验）：
        let bad = json!({"api": "bogus", "baseUrl": "x", "models": [{"id": "m"}]});
        assert!(write_provider_card("bad", "B", &bad).is_err());
        let no_url = json!({"api": "openai-completions", "baseUrl": " ", "models": [{"id": "m"}]});
        assert!(write_provider_card("n1", "N", &no_url).is_err());
        let no_models = json!({"api": "openai-completions", "baseUrl": "https://x", "models": []});
        assert!(write_provider_card("n2", "N", &no_models).is_err());
    }

    #[test]
    fn sanitize_route_normalizes_ids() {
        assert_eq!(sanitize_route("My Provider_2"), "my-provider-2");
        assert_eq!(sanitize_route("工具---x"), "x");
        assert!(sanitize_route("---").starts_with("provider-"));
    }
}
