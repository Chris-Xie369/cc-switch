# DeepSeek Harness AppType（R2）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task（本项目惯例：可主会话直接执行）。Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** cc-switch 新增 `deepseek-harness` additive AppType：供应商卡 CRUD `~/.dsh/settings.yaml` + `.credentials.yaml refs:`，聚合槽位自动/手动同步。

**Architecture:** 后端新模块 `dsh_config.rs`（路径参数化纯函数 + 薄包装），AppType 接线复用 Hermes/OpenCode additive 模式，聚合同步挂 `apply_provider` 的 model_picker 旁。前端新 `DshProviderForm`（5 字段）+ ProviderList 同步按钮。i18n 仅 en/zh。

**Tech Stack:** Rust（serde_yaml 0.9 已在库）、React/TS、Tauri commands。

## Global Constraints（来自 spec）

- 归属：只拥有 `llm-pi-ai.providers.<route>` 各键与 `refs:<envName>`；文件其余（`records:`、harness 写的其他键）原样保留；接受 serde_yaml 丢注释（D2）
- 聚合 route 固定 `cc-switch` / env 名 `CC_SWITCH_GATEWAY_TOKEN`；手动卡 env 名 `CC_SWITCH_DSH_<ROUTE 大写>`（D1=B）
- i18n 仅 en/zh（D3）；DB 零迁移（app_type TEXT 存 `deepseek-harness`）
- 家目录解析序：cc-switch 设置覆盖 → env `DSH_HOME` → `~/.dsh`
- 提交信息不含署名行；分支 `fix/profile-merge` 只推 origin

---

### Task 1: dsh_config.rs 核心（YAML 适配器，路径参数化）

**Files:**
- Create: `src-tauri/src/dsh_config.rs`
- Modify: `src-tauri/src/lib.rs`（mod 声明，跟随 `mod hermes_config;` 行）
- Test: 同文件 `#[cfg(test)] mod tests`

**Interfaces（后续任务依赖）:**
- Produces:
  - `pub fn get_dsh_dir() -> Result<PathBuf, AppError>`（覆盖序见 Global Constraints）
  - `pub fn read_providers_at(dir: &Path) -> Result<IndexMap<String, serde_json::Value>, AppError>`
  - `pub fn upsert_provider_at(dir: &Path, route: &str, spec: &DshProviderSpec) -> Result<(), AppError>`
  - `pub fn remove_provider_at(dir: &Path, route: &str) -> Result<(), AppError>`
  - `pub fn upsert_credential_ref_at(dir: &Path, name: &str, secret: &str) -> Result<(), AppError>`
  - `pub fn remove_credential_ref_at(dir: &Path, name: &str, only_if_matches: Option<&str>) -> Result<(), AppError>`
  - `pub struct DshProviderSpec { pub display_name: String, pub api: String, pub base_url: String, pub api_key_env: String, pub models: Vec<DshModelEntry> }`（`DshModelEntry { pub id: String, pub name: String }`）
  - `pub fn sync_aggregate_entry_at(dir: &Path, token: &str, routes: &[crate::claude_desktop_config::ResolvedModelRoute]) -> Result<(), AppError>`

- [ ] **Step 1: 写失败测试**（tempdir：upsert→read 回读断言；remove 保留他人键；ref 增删；records 保留；聚合同步产物形状与 R1 手工版一致）

```rust
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
            models: vec![DshModelEntry { id: "m1".into(), name: "M One".into() }],
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
    }

    #[test]
    fn remove_preserves_other_routes_and_root_keys() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("settings.yaml"),
            "other-top: 1\nllm-pi-ai:\n  providers:\n    keep: {}\n    foo: {}\n").unwrap();
        remove_provider_at(dir.path(), "foo").unwrap();
        let t = std::fs::read_to_string(dir.path().join("settings.yaml")).unwrap();
        assert!(t.contains("other-top: 1") && t.contains("keep") && !t.contains("foo:"));
    }

    #[test]
    fn credential_refs_and_records_preserved() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".credentials.yaml"),
            "version: 1\nrefs:\n  OTHER: k1\nrecords:\n  - kind: oauth\n").unwrap();
        upsert_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", "sk-1").unwrap();
        upsert_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", "sk-2").unwrap();
        remove_credential_ref_at(dir.path(), "CC_SWITCH_DSH_FOO", Some("sk-2")).unwrap();
        let t = std::fs::read_to_string(dir.path().join(".credentials.yaml")).unwrap();
        assert!(t.contains("OTHER: k1") && t.contains("records") && !t.contains("CC_SWITCH_DSH_FOO"));
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
        }];
        sync_aggregate_entry_at(dir.path(), "tok", &routes).unwrap();
        let providers = read_providers_at(dir.path()).unwrap();
        let ccsw = providers.get("cc-switch").unwrap().as_object().unwrap();
        assert_eq!(ccsw["api"], json!("anthropic-messages"));
        assert_eq!(ccsw["baseURL"], json!("http://127.0.0.1:15721/claude-desktop"));
        assert_eq!(ccsw["apiKeyEnv"], json!("CC_SWITCH_GATEWAY_TOKEN"));
        assert_eq!(ccsw["models"][0]["id"], json!("claude-fable-1"));
        let t = std::fs::read_to_string(dir.path().join(".credentials.yaml")).unwrap();
        assert!(t.contains("CC_SWITCH_GATEWAY_TOKEN: tok"));
    }
}
```

- [ ] **Step 2: 跑红** `cargo test --release --lib dsh_config`（模块不存在编译失败即红）
- [ ] **Step 3: 实现**

```rust
//! DeepSeek Harness（pi-ai 下游）的 settings.yaml / .credentials.yaml 薄适配器。
//! cc-switch 只拥有 llm-pi-ai.providers.<route> 的键与 refs:<name>；其余原样保留。

use crate::error::AppError;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use serde_yaml::Mapping;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct DshModelEntry { pub id: String, pub name: String }

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

pub fn get_dsh_dir() -> Result<PathBuf, AppError> {
    if let Some(p) = crate::settings::get_dsh_override_dir() { return Ok(p); }
    if let Some(env) = std::env::var_os("DSH_HOME") {
        if !env.is_empty() {
            let p = PathBuf::from(env);
            if !p.is_absolute() {
                return Err(AppError::InvalidInput("DSH_HOME must be absolute".into()));
            }
            return Ok(p);
        }
    }
    Ok(crate::config::get_home_dir().join(".dsh"))
}

fn read_yaml_mapping(path: &Path, what: &str) -> Result<Mapping, AppError> {
    if !path.exists() { return Ok(Mapping::new()); }
    let text = std::fs::read_to_string(path)
        .map_err(|e| AppError::io(path, e))?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|e| {
        AppError::Config(format!("DeepSeek Harness {what} 解析失败 ({}): {e}", path.display()))
    })?;
    match doc {
        serde_yaml::Value::Mapping(m) => Ok(m),
        serde_yaml::Value::Null => Ok(Mapping::new()),
        _ => Err(AppError::Config(format!("DeepSeek Harness {what} 根节点必须是映射"))),
    }
}

fn write_yaml_mapping(path: &Path, doc: &serde_yaml::Value, what: &str) -> Result<(), AppError> {
    let text = serde_yaml::to_string(doc)
        .map_err(|e| AppError::Config(format!("DeepSeek Harness {what} 序列化失败: {e}")))?;
    std::fs::write(path, text).map_err(|e| AppError::io(path, e))
}

fn yaml_to_json(v: &serde_yaml::Value) -> Value { serde_json::to_value(v).unwrap_or(Value::Null) }
fn json_to_yaml(v: &Value) -> serde_yaml::Value {
    serde_yaml::to_value(v).unwrap_or(serde_yaml::Value::Null)
}

fn providers_map_mut<'a>(root: &'a mut Mapping) -> &'a mut Mapping {
    let llm = root.entry(serde_yaml::Value::from("llm-pi-ai"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if !llm.is_mapping() { *llm = serde_yaml::Value::Mapping(Mapping::new()); }
    let llm_map = llm.as_mut().unwrap();
    let prov = llm_map.entry(serde_yaml::Value::from("providers"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if !prov.is_mapping() { *prov = serde_yaml::Value::Mapping(Mapping::new()); }
    prov.as_mut().unwrap()
}

pub fn read_providers_at(dir: &Path) -> Result<IndexMap<String, Value>, AppError> {
    // IndexMap 保持 YAML 映射顺序；serde_yaml Mapping 本身有序
    let root = read_yaml_mapping(&dir.join(SETTINGS_FILE), "settings")?;
    let mut out = IndexMap::new();
    if let Some(serde_yaml::Value::Mapping(prov)) =
        root.get(&serde_yaml::Value::from("llm-pi-ai"))
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

pub fn upsert_provider_at(dir: &Path, route: &str, spec: &DshProviderSpec) -> Result<(), AppError> {
    let value = json!({
        "displayName": spec.display_name,
        "api": spec.api,
        "baseURL": spec.base_url,
        "apiKeyEnv": spec.api_key_env,
        "models": spec.models.iter().map(|m| json!({"id": m.id, "name": m.name}))
            .collect::<Vec<_>>(),
    });
    let path = dir.join(SETTINGS_FILE);
    let mut root = read_yaml_mapping(&path, "settings")?;
    providers_map_mut(&mut root)
        .insert(serde_yaml::Value::from(route), json_to_yaml(&value));
    let doc = serde_yaml::Value::Mapping(root);
    write_yaml_mapping(&path, &doc, "settings")
}

pub fn remove_provider_at(dir: &Path, route: &str) -> Result<(), AppError> {
    let path = dir.join(SETTINGS_FILE);
    let mut root = read_yaml_mapping(&path, "settings")?;
    let empty = {
        let prov = providers_map_mut(&mut root);
        prov.remove(serde_yaml::Value::from(route));
        prov.is_empty()
    };
    if empty {
        // providers 空则连 llm-pi-ai 一并收走，避免留空壳
        if let Some(llm) = root.get_mut(&serde_yaml::Value::from("llm-pi-ai")) {
            if let Some(m) = llm.as_mut() {
                m.remove(serde_yaml::Value::from("providers"));
                if m.is_empty() { root.remove(&serde_yaml::Value::from("llm-pi-ai")); }
            }
        }
    }
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "settings")
}

pub fn upsert_credential_ref_at(dir: &Path, name: &str, secret: &str) -> Result<(), AppError> {
    let path = dir.join(CREDENTIALS_FILE);
    let mut root = read_yaml_mapping(&path, "credentials")?;
    let refs = root.entry(serde_yaml::Value::from("refs"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if !refs.is_mapping() { *refs = serde_yaml::Value::Mapping(Mapping::new()); }
    refs.as_mut().unwrap()
        .insert(serde_yaml::Value::from(name), serde_yaml::Value::from(secret));
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "credentials")
}

pub fn remove_credential_ref_at(dir: &Path, name: &str, only_if_matches: Option<&str>) -> Result<(), AppError> {
    let path = dir.join(CREDENTIALS_FILE);
    let mut root = read_yaml_mapping(&path, "credentials")?;
    if let Some(serde_yaml::Value::Mapping(refs)) = root.get_mut(&serde_yaml::Value::from("refs")) {
        let removable = match (only_if_matches, refs.get(&serde_yaml::Value::from(name))) {
            (Some(expect), Some(v)) => v.as_str() == Some(expect),
            _ => only_if_matches.is_none(),
        };
        if removable { refs.remove(serde_yaml::Value::from(name)); }
    }
    write_yaml_mapping(&path, &serde_yaml::Value::Mapping(root), "credentials")
}

pub fn sync_aggregate_entry_at(dir: &Path, token: &str, routes: &[crate::claude_desktop_config::ResolvedModelRoute]) -> Result<(), AppError> {
    let spec = DshProviderSpec {
        display_name: "CC Switch Aggregate".into(),
        api: "anthropic-messages".into(),
        base_url: "http://127.0.0.1:15721/claude-desktop".into(),
        api_key_env: "CC_SWITCH_GATEWAY_TOKEN".into(),
        models: routes.iter().map(|r| DshModelEntry {
            id: r.route_id.clone(),
            name: r.label_override.clone().unwrap_or_else(|| r.upstream_model.clone()),
        }).collect(),
    };
    upsert_provider_at(dir, "cc-switch", &spec)?;
    upsert_credential_ref_at(dir, "CC_SWITCH_GATEWAY_TOKEN", token)
}
```

- [ ] **Step 4: 跑绿** `cargo test --release --lib dsh_config` → 4 passed
- [ ] **Step 5: lib.rs 挂模块**（`mod dsh_config;` 按 hermes_config 位置）+ `cargo check` 0
- [ ] **Step 6: Commit** `feat(dsh): yaml adapter for settings.yaml and credential refs`

**注意**：`ResolvedModelRoute` 字段以 `claude_desktop_config.rs:116-123` 现版为准（含 `max_effort`）；serde_yaml Mapping 的 entry API 如不可用（0.9），改用 `mapping.insert(key.clone(), …)` + 手动 get_mut 组合，测试不变。

---

### Task 2: AppType 接线（枚举 + services CRUD）

**Files:**
- Modify: `src-tauri/src/app_config.rs`（枚举 + as_str/from_str + is_additive_mode + 可见性结构两处 `pi` 字段旁加 `deepseek_harness`）
- Modify: `src-tauri/src/services/provider/live.rs`（`write_live_for_state` 加 arm → `dsh_config::upsert_provider_at(...)` + ref 写入；新增 `remove_dsh_provider_from_live`）
- Modify: `src-tauri/src/services/provider/mod.rs`（remove 分支加 arm，仿 `remove_hermes_provider_from_live` 调用点）
- Test: `src-tauri/src/services/provider/mod.rs` tests + dsh_config tests

**Interfaces:**
- Consumes: Task 1 全部函数；`Provider.settings_config` 形状 `{"displayName","api","baseURL","apiKey","models":[{"id","name"}]}`
- Produces: `AppType::DeepSeekHarness`（as_str `"deepseek-harness"`）、`remove_dsh_provider_from_live(provider_id, display_name)`（供 remove 流与命令层用）

- [ ] **Step 1: settings.rs 加 `get_dsh_override_dir()`**（仿 `get_pi_override_dir`，settings 表键 `dsh_override_dir`——若 settings 结构是固定字段则加字段，默认 None）
- [ ] **Step 2: app_config.rs**：
  - 枚举 `DeepSeekHarness`
  - `as_str` → `"deepseek-harness"`（431 行区）
  - `from_str` → `"deepseek-harness" => Ok(AppType::DeepSeekHarness)`（487 行区）
  - `is_additive_mode` matches 加 `| AppType::DeepSeekHarness`（441 行区）
  - 可见性两结构加 `pub deepseek_harness: bool` + match arm（40/56、135/151 行区，默认 `false`——用户在设置里显式开启，参考 appVisibility 机制）
- [ ] **Step 3: live.rs `write_live_for_state` 加 arm**（ClaudeDesktop arm 旁）：

```rust
if matches!(app_type, AppType::DeepSeekHarness) {
    let dir = crate::dsh_config::get_dsh_dir()?;
    let route = sanitize_dsh_route(&provider.id);
    let env_name = format!("CC_SWITCH_DSH_{}", route.to_uppercase().replace('-', "_"));
    let models = provider.settings_config.get("models").cloned().unwrap_or(json!([]));
    let models: Vec<DshModelEntry> = serde_json::from_value(models)
        .map_err(|e| AppError::Config(format!("DSH 模型列表格式无效: {e}")))?;
    let spec = DshProviderSpec {
        display_name: str_field(&provider.settings_config, "displayName", &provider.name),
        api: match str_field(&provider.settings_config, "api", "openai-completions").as_str() {
            "anthropic-messages" | "openai-responses" | "openai-completions" => s,
            other => return Err(AppError::Config(format!("DSH api 不支持: {other}"))),
        },
        base_url: str_field(&provider.settings_config, "baseURL", ""),
        api_key_env: env_name.clone(),
        models,
    };
    if spec.base_url.is_empty() || spec.models.is_empty() {
        return Err(AppError::Config("DSH 供应商需要 baseURL 与至少一个模型".into()));
    }
    crate::dsh_config::upsert_provider_at(&dir, &route, &spec)?;
    if let Some(key) = provider.settings_config.get("apiKey").and_then(Value::as_str) {
        if !key.is_empty() {
            crate::dsh_config::upsert_credential_ref_at(&dir, &env_name, key)?;
        }
    }
    return Ok(());
}
```

  `str_field`/`sanitize_dsh_route`（小写连字符，非法字符转 `-`，空则 `provider-<id前8>`）作为本文件私有 helper。
- [ ] **Step 4: remove 流**：`remove_dsh_provider_from_live(provider_id: &str, display_name: &str)`——`remove_provider_at(dir, route)` + `remove_credential_ref_at(dir, env, None)`；在 services/provider/mod.rs 的 remove 分支（`remove_opencode_provider_from_live` 调用点区）加 arm
- [ ] **Step 5: 测试**：dsh_config tests 加「write_live_for_state 经 DB 状态跑通」的集成用例（参照 `import_hermes_providers_from_live_updates_existing_provider_from_live` 的 with_test_home 模式，但用 TEST 目录覆盖；若 with_test_home 不支持 DSH 目录覆盖，则此步降级为纯 dsh_config 层测试 + Task 6 手工验收，并在账本记录）
- [ ] **Step 6: `cargo test --release --lib dsh` + `cargo check`** 全绿
- [ ] **Step 7: Commit** `feat(dsh): AppType wiring with additive CRUD`

---

### Task 3: 聚合同步挂钩 + 命令层

**Files:**
- Modify: `src-tauri/src/claude_desktop_config.rs:143`（apply_provider 里 model_picker 同步后并排挂 DSH，warn-only）
- Create: `src-tauri/src/commands/dsh.rs`
- Modify: `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`（注册）

**Interfaces:**
- Produces: tauri command `sync_dsh_aggregate() -> Result<(), String>`——读当前 claude-desktop 聚合供应商 + 网关 token，调 `sync_aggregate_entry_at`

- [ ] **Step 1: claude_desktop_config.rs 挂钩**（143 行 `sync_cli_model_picker` 之后）：

```rust
    if let Err(err) = crate::dsh_config::sync_dsh_aggregate_now(db, routes.as_deref()) {
        log::warn!("DeepSeek Harness 聚合同步失败（下次 apply 自愈）: {err}");
    }
```

  `dsh_config` 内实现 `sync_dsh_aggregate_now(db, routes)`：routes 为 None（非聚合切换）时读 DB 当前 claude-desktop 聚合判断是否已配置过 cc-switch route——配置过才重写（槽位可能变了），没配置过则跳过（用户未启用 DSH 接入）；routes 为 Some 时直接同步。token 用 `get_or_create_gateway_token(db)`。
- [ ] **Step 2: commands/dsh.rs**：

```rust
#[tauri::command]
pub fn sync_dsh_aggregate() -> Result<(), String> {
    let dir = crate::dsh_config::get_dsh_dir().map_err(|e| e.to_string())?;
    let db = crate::database::Database::new_default()
        .map_err(|e| e.to_string())?;
    crate::dsh_config::sync_dsh_aggregate_now(&db, None).map_err(|e| e.to_string())
}
```

  （DB 句柄获取方式以 commands/hermes.rs 同款为准——若走 AppState 则改为 `state: tauri::State<AppState>`；实现时对齐。）
- [ ] **Step 3: 注册**：commands/mod.rs `pub mod dsh;`、lib.rs invoke_handler 加 `commands::dsh::sync_dsh_aggregate`
- [ ] **Step 4: 测试**：dsh_config tests 加「sync_dsh_aggregate_now：无聚合且未配置→不动文件；已配置→刷新」两例
- [ ] **Step 5: cargo test + check 绿；Commit** `feat(dsh): aggregate slot sync on apply and via command`

---

### Task 4: 前端 AppId + 注册 + i18n

**Files:**
- Modify: `src/lib/api/types.ts:1-11`（union 加 `"deepseek-harness"` + 38-40 区默认可见性对象加键 `false`）
- Modify: `src/config/appConfig.tsx`（hermes 的 7 个触点逐一加同款：27/52/92/123 顺序与分组数组、85 工具类型联合、203 app 配置对象 `deepseekHarness: { icon: "deepseek", … }`——图标用 lucide 现有 `Bot` 或同款占位，对齐 hermes 对象形状）
- Modify: `src/i18n/locales/en.json`、`zh.json`（apps 列表名 "DeepSeek Harness"、apps.deepseekHarness 段——键名与 hermes 段对齐后翻译）
- Test: `npx tsc --noEmit` 0

- [ ] **Step 1-3**: 三文件修改（逐触点对齐 hermes；zh 文案：「DeepSeek Harness」「同步聚合模型到 DeepSeek Harness」等）
- [ ] **Step 4: tsc + vitest 冒烟（appConfig 相关既有测试）**
- [ ] **Step 5: Commit** `feat(dsh): frontend app registration and i18n`

---

### Task 5: DshProviderForm + 分派 + 同步按钮

**Files:**
- Create: `src/components/providers/forms/DshProviderForm.tsx`
- Modify: `src/components/providers/forms/ProviderForm.tsx:324` 区（`appId === "deepseek-harness"` 分派）+ import
- Modify: `src/lib/api/providers.ts`（`syncDshAggregate(): Promise<void>` → invoke `sync_dsh_aggregate`）
- Modify: `src/components/providers/ProviderList.tsx:191-200` 区（appId 匹配加同步动作：调用 → toast 成功 → refetch；文案 i18n）
- Test: `tests/components/DshProviderForm.test.tsx`（新）

**Interfaces:**
- settingsConfig JSON 形状（与 Task 2 一致）：`{"displayName","api","baseURL","apiKey","models":[{"id","name"}]}`

- [ ] **Step 1: 表单组件**（受控 props 对齐 ProviderForm 约定：initialData/onSubmit/onCancel；字段：displayName 文本、api 三选 Select、baseURL 文本、apiKey 密码框（编辑时留空=不改）、models 可增删行（两输入）+ 从 baseURL 拉取按钮可选**不做**——YAGNI；提交时组装 settingsConfig）
- [ ] **Step 2: 测试**（参照 AddProviderDialog.test 模式）：渲染断言字段、提交断言 payload 形状、必填校验（baseURL/模型非空）
- [ ] **Step 3: tsc + vitest 绿**
- [ ] **Step 4: Commit** `feat(dsh): provider form and aggregate sync button`

---

### Task 6: 全量验证 + 构建部署 + 账本

- [ ] cargo test --release 全量（对照既有 4 环境性失败名单）
- [ ] tsc + vitest 全量
- [ ] 构建（worktree 流程：feat/dsh-apptype 分支）→ install-local.bat 部署 → 三重校验
- [ ] **手工 E2E**：①DSH 页建卡（openai-completions 指向任一真实网关+key+1 模型）→ settings.yaml/refs 出现 → harness Models 页可见；②删卡 → 键消失他人保留；③Claude Desktop 切换聚合 → DSH cc-switch route 槽位刷新；④同步按钮 → 立即刷新
- [ ] 账本 + spec 状态改「已交付」+ 合回 fix/profile-merge 推送

## Self-Review

- Spec 覆盖：D1=B（Task 2/5 CRUD）、聚合同步（Task 3）、归属保留（Task 1 测试）、D2 注释（Global Constraints + 实现即 round-trip）、D3 i18n（Task 4）、上游化（收尾时另提）✓
- 占位扫描：Task 2 Step 3 的 `str_field` helper 有定义说明；Task 3 Step 2 DB 句柄「对齐 hermes」为锚点跟随而非占位 ✓
- 类型一致：DshProviderSpec/DshModelEntry 全计划同名；settingsConfig JSON 键 Task 2/5 一致 ✓
