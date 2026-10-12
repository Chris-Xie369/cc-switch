//! DeepSeek Harness（pi-ai 下游）的 settings.yaml / .credentials.yaml 薄适配器。
//!
//! cc-switch 只拥有 `llm-pi-ai.providers.<route>` 的各键与 `.credentials.yaml`
//! `refs:<name>` 条目；文件其余内容（harness Models 页写入的键、`records:`
//! OAuth 区）原样保留。serde_yaml round-trip 不保留注释——该文件本就由
//! harness 自己的编辑器整写，非注释承载（spec D2）。

use crate::error::AppError;
use indexmap::IndexMap;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct DshModelEntry {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DshProviderSpec {
    pub display_name: String,
    pub api: String,
    pub base_url: String,
    pub api_key_env: String,
    pub models: Vec<DshModelEntry>,
}

const SETTINGS_FILE: &str = "settings.yaml";
const CREDENTIALS_FILE: &str = ".credentials.yaml";
/// 聚合供应商在 settings.yaml 里的固定 route 名（R1 手工版同名，幂等续接）。
pub const AGGREGATE_ROUTE: &str = "cc-switch";
/// 聚合供应商的凭据引用名（写入 `.credentials.yaml refs:`）。
pub const AGGREGATE_TOKEN_ENV: &str = "CC_SWITCH_GATEWAY_TOKEN";

pub fn get_dsh_dir() -> Result<std::path::PathBuf, AppError> {
    if let Some(path) = crate::settings::get_dsh_override_dir() {
        return Ok(path);
    }
    if let Some(env) = std::env::var_os("DSH_HOME") {
        if !env.is_empty() {
            let path = std::path::PathBuf::from(&env);
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

fn read_yaml_mapping(
    path: &std::path::Path,
    what: &str,
) -> Result<serde_yaml::Mapping, AppError> {
    if !path.exists() {
        return Ok(serde_yaml::Mapping::new());
    }
    let text = std::fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|e| {
        AppError::Config(format!(
            "DeepSeek Harness {what} 解析失败 ({}): {e}",
            path.display()
        ))
    })?;
    match doc {
        serde_yaml::Value::Mapping(m) => Ok(m),
        serde_yaml::Value::Null => Ok(serde_yaml::Mapping::new()),
        _ => Err(AppError::Config(format!(
            "DeepSeek Harness {what} 根节点必须是映射"
        ))),
    }
}

fn write_yaml_mapping(
    path: &std::path::Path,
    doc: &serde_yaml::Value,
    what: &str,
) -> Result<(), AppError> {
    let text = serde_yaml::to_string(doc)
        .map_err(|e| AppError::Config(format!("DeepSeek Harness {what} 序列化失败: {e}")))?;
    std::fs::write(path, text).map_err(|e| AppError::io(path, e))
}

fn yaml_to_json(v: &serde_yaml::Value) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn json_to_yaml(v: &Value) -> serde_yaml::Value {
    serde_yaml::to_value(v).unwrap_or(serde_yaml::Value::Null)
}

fn providers_map_mut(root: &mut serde_yaml::Mapping) -> &mut serde_yaml::Mapping {
    let key = serde_yaml::Value::from("llm-pi-ai");
    let llm = root
        .entry(key.clone())
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    if !llm.is_mapping() {
        *llm = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    let llm_map = llm.as_mapping_mut().expect("just made a mapping");
    let prov_key = serde_yaml::Value::from("providers");
    let prov = llm_map
        .entry(prov_key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    if !prov.is_mapping() {
        *prov = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    prov.as_mapping_mut().expect("just made a mapping")
}

pub fn read_providers_at(dir: &std::path::Path) -> Result<IndexMap<String, Value>, AppError> {
    let root = read_yaml_mapping(&dir.join(SETTINGS_FILE), "settings")?;
    let mut out = IndexMap::new();
    let llm_key = serde_yaml::Value::from("llm-pi-ai");
    if let Some(serde_yaml::Value::Mapping(prov)) = root
        .get(&llm_key)
        .and_then(|v| v.get(serde_yaml::Value::from("providers")))
    {
        for (k, v) in prov {
            if let Some(name) = k.as_str() {
                out.insert(name.to_string(), yaml_to_json(v));
            }
        }
    }
    Ok(out)
}

pub fn upsert_provider_at(
    dir: &std::path::Path,
    route: &str,
    spec: &DshProviderSpec,
) -> Result<(), AppError> {
    let value = json!({
        "displayName": spec.display_name,
        "api": spec.api,
        "baseURL": spec.base_url,
        "apiKeyEnv": spec.api_key_env,
        "models": spec.models
            .iter()
            .map(|m| json!({"id": m.id, "name": m.name}))
            .collect::<Vec<_>>(),
    });
    let path = dir.join(SETTINGS_FILE);
    let mut root = read_yaml_mapping(&path, "settings")?;
    providers_map_mut(&mut root).insert(serde_yaml::Value::from(route), json_to_yaml(&value));
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "settings")
}

pub fn remove_provider_at(dir: &std::path::Path, route: &str) -> Result<(), AppError> {
    let path = dir.join(SETTINGS_FILE);
    let mut root = read_yaml_mapping(&path, "settings")?;
    let empty = {
        let prov = providers_map_mut(&mut root);
        prov.remove(serde_yaml::Value::from(route));
        prov.is_empty()
    };
    if empty {
        // providers 空则连 llm-pi-ai 一并收走，避免留空壳
        let llm_key = serde_yaml::Value::from("llm-pi-ai");
        if let Some(llm_map) = root.get_mut(&llm_key).and_then(|v| v.as_mapping_mut()) {
            llm_map.remove(serde_yaml::Value::from("providers"));
            if llm_map.is_empty() {
                root.remove(&llm_key);
            }
        }
    }
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "settings")
}

pub fn upsert_credential_ref_at(
    dir: &std::path::Path,
    name: &str,
    secret: &str,
) -> Result<(), AppError> {
    let path = dir.join(CREDENTIALS_FILE);
    let mut root = read_yaml_mapping(&path, "credentials")?;
    let refs_key = serde_yaml::Value::from("refs");
    let refs = root
        .entry(refs_key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    if !refs.is_mapping() {
        *refs = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    refs.as_mapping_mut()
        .expect("just made a mapping")
        .insert(serde_yaml::Value::from(name), serde_yaml::Value::from(secret));
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "credentials")
}

pub fn remove_credential_ref_at(
    dir: &std::path::Path,
    name: &str,
    only_if_matches: Option<&str>,
) -> Result<(), AppError> {
    let path = dir.join(CREDENTIALS_FILE);
    let mut root = read_yaml_mapping(&path, "credentials")?;
    let refs_key = serde_yaml::Value::from("refs");
    if let Some(serde_yaml::Value::Mapping(refs)) = root.get_mut(&refs_key) {
        let name_key = serde_yaml::Value::from(name);
        let removable = match (only_if_matches, refs.get(&name_key)) {
            (Some(expected), Some(v)) => v.as_str() == Some(expected),
            _ => only_if_matches.is_none(),
        };
        if removable {
            refs.remove(name_key);
        }
    }
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "credentials")
}

pub fn sync_aggregate_entry_at(
    dir: &std::path::Path,
    token: &str,
    routes: &[crate::claude_desktop_config::ResolvedModelRoute],
) -> Result<(), AppError> {
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
            })
            .collect(),
    };
    upsert_provider_at(dir, AGGREGATE_ROUTE, &spec)?;
    upsert_credential_ref_at(dir, AGGREGATE_TOKEN_ENV, token)
}

/// 是否已配置过聚合 route（决定「用户未启用 DSH 接入」时 apply 不写文件）。
pub fn aggregate_entry_exists(dir: &std::path::Path) -> Result<bool, AppError> {
    Ok(read_providers_at(dir)?.contains_key(AGGREGATE_ROUTE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

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
    fn upsert_then_read_round_trip() {
        let dir = TempDir::new().unwrap();
        upsert_provider_at(dir.path(), "foo", &spec("CC_SWITCH_DSH_FOO")).unwrap();
        let providers = read_providers_at(dir.path()).unwrap();
        let foo = providers.get("foo").expect("foo route").as_object().unwrap();
        assert_eq!(foo["api"], json!("openai-completions"));
        assert_eq!(foo["baseURL"], json!("https://x.example"));
        assert_eq!(foo["apiKeyEnv"], json!("CC_SWITCH_DSH_FOO"));
        assert_eq!(foo["displayName"], json!("Foo"));
        assert_eq!(foo["models"][0]["id"], json!("m1"));
        assert_eq!(foo["models"][0]["name"], json!("M One"));
    }

    #[test]
    fn upsert_preserves_other_routes_and_root_keys() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            "other-top: 1\nllm-pi-ai:\n  providers:\n    keep:\n      api: openai-responses\n",
        )
        .unwrap();
        upsert_provider_at(dir.path(), "foo", &spec("CC_SWITCH_DSH_FOO")).unwrap();
        let providers = read_providers_at(dir.path()).unwrap();
        assert!(providers.contains_key("keep"));
        assert_eq!(providers["keep"]["api"], json!("openai-responses"));
        let text = std::fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap();
        assert!(text.contains("other-top: 1"));
    }

    #[test]
    fn remove_preserves_other_routes_and_root_keys() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            "other-top: 1\nllm-pi-ai:\n  providers:\n    keep:\n      api: x\n    foo:\n      api: y\n",
        )
        .unwrap();
        remove_provider_at(dir.path(), "foo").unwrap();
        let text = std::fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap();
        assert!(text.contains("other-top: 1"));
        assert!(text.contains("keep"));
        assert!(!text.contains("foo"));
    }

    #[test]
    fn remove_last_provider_prunes_llm_section() {
        let dir = TempDir::new().unwrap();
        upsert_provider_at(dir.path(), "foo", &spec("CC_SWITCH_DSH_FOO")).unwrap();
        remove_provider_at(dir.path(), "foo").unwrap();
        let text = std::fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap();
        assert!(!text.contains("llm-pi-ai"));
    }

    #[test]
    fn credential_refs_upsert_and_remove_preserving_records() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(CREDENTIALS_FILE),
            "version: 1\nrefs:\n  OTHER: k1\nrecords:\n  - kind: oauth\n",
        )
        .unwrap();
        upsert_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", "sk-1").unwrap();
        // 覆盖写同值更新
        upsert_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", "sk-2").unwrap();
        // 仅当匹配才删：值不符不动
        remove_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", Some("wrong")).unwrap();
        let text = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(text.contains("CC_SWITCH_DSH_FOO: sk-2"));
        // 值匹配删除
        remove_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", Some("sk-2")).unwrap();
        let text = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(text.contains("OTHER: k1"));
        assert!(text.contains("records"));
        assert!(!text.contains("CC_SWITCH_DSH_FOO"));
    }

    #[test]
    fn aggregate_entry_shape_matches_r1() {
        let dir = TempDir::new().unwrap();
        let routes = vec![crate::claude_desktop_config::ResolvedModelRoute {
            route_id: "claude-fable-1".into(),
            upstream_model: "glm-5.3".into(),
            label_override: Some("Zhipu · glm-5.3".into()),
            supports_1m: true,
            max_effort: None,
            tier: None,
        }];
        sync_aggregate_entry_at(dir.path(), "tok", &routes).unwrap();
        let providers = read_providers_at(dir.path()).unwrap();
        let ccsw = providers.get(AGGREGATE_ROUTE).unwrap().as_object().unwrap();
        assert_eq!(ccsw["api"], json!("anthropic-messages"));
        assert_eq!(
            ccsw["baseURL"],
            json!("http://127.0.0.1:15721/claude-desktop")
        );
        assert_eq!(ccsw["apiKeyEnv"], json!(AGGREGATE_TOKEN_ENV));
        assert_eq!(ccsw["displayName"], json!("CC Switch Aggregate"));
        assert_eq!(ccsw["models"][0]["id"], json!("claude-fable-1"));
        assert_eq!(ccsw["models"][0]["name"], json!("Zhipu · glm-5.3"));
        let text = std::fs::read_to_string(dir.path().join(CREDENTIALS_FILE)).unwrap();
        assert!(text.contains(&format!("{AGGREGATE_TOKEN_ENV}: tok")));
        assert!(aggregate_entry_exists(dir.path()).unwrap());
    }
}
