# CLI 模型选择器（modelPicker）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 聚合供应商槽位自动写入 `~/.claude/settings.json` 的 `modelPicker`，让 Claude Code CLI 的 `/model` 选择器一次列出多家模型。

**Architecture:** 新增单一职责模块 `model_picker.rs`（纯函数生成 + 带存证的写/删同步），挂在 `apply_provider`（Claude Desktop live 写入唯一漏斗）上，与 profile `inferenceModels` 同寿命。零前端、零代理改动。

**Tech Stack:** Rust（serde_json、rusqlite settings kv）、cargo test --release。

**Spec:** `docs/superpowers/specs/2026-10-01-cli-model-picker-design.md`（决策 D1–D5 以 spec 为准）

## Global Constraints

- 测试必须用 `TempHome`（pin `CC_SWITCH_TEST_HOME`/`HOME`/`USERPROFILE`/`LOCALAPPDATA`）+ `#[serial]`，**绝不写真实 `~/.claude/settings.json` 或真实 3P profile**（2026-09-22 事故教训）。
- Rust 测试一律 `cargo test --release`（debug 树 18G 教训，见 UPSTREAM-SYNC §5）。
- 中文注释仅限「约束/为什么」；不写叙述性注释。cargo fmt 必须 0 diff。
- behavesAs 档位表逐字：fable→`claude-fable-5`、opus→`claude-opus-4-8`、sonnet→`claude-sonnet-4-6`、haiku→`claude-haiku-4-5`。
- `replaceBuiltInOptions` 恒为 `false`。
- 1M 行 label 后缀逐字：` · 1M`（前后各一空格）。

---

### Task 1: `model_picker` 模块（生成 + 带存证同步）

**Files:**
- Create: `src-tauri/src/model_picker.rs`
- Modify: `src-tauri/src/lib.rs`（第 1 行 `mod aggregate;` 之后加 `mod model_picker;`）

**Interfaces:**
- Consumes: `crate::claude_desktop_config::ResolvedModelRoute`（字段：`route_id: String`、`upstream_model: String`、`label_override: Option<String>`、`supports_1m: bool`、`max_effort: Option<String>`、`tier: Option<String>`）；`crate::config::{get_claude_settings_path, read_json_file, write_json_file}`；`crate::database::Database::{get_setting, set_setting}`。
- Produces（Task 2 依赖，签名必须逐字一致）:
  - `pub fn model_picker_value(routes: &[ResolvedModelRoute]) -> serde_json::Value`
  - `pub(crate) fn sync_cli_model_picker(db: &Database, routes: Option<&[ResolvedModelRoute]>) -> Result<(), AppError>`
  - `pub(crate) fn sync_cli_model_picker_at(db: &Database, path: &Path, routes: Option<&[ResolvedModelRoute]>) -> Result<(), AppError>`

- [ ] **Step 1: 写失败测试**（整文件见下）

`model_picker.rs` 顶部实现区先放桩（`todo!()`），测试区一次给全；本任务内按红→绿推进。

```rust
// src-tauri/src/model_picker.rs — 测试区（与实现同文件，#[cfg(test)] mod tests）
use super::*;
use crate::claude_desktop_config::ResolvedModelRoute;
use serde_json::json;

fn route(route_id: &str, tier: &str, label: Option<&str>, supports_1m: bool) -> ResolvedModelRoute {
    ResolvedModelRoute {
        route_id: route_id.to_string(),
        upstream_model: "glm-5.3".to_string(),
        label_override: label.map(str::to_string),
        supports_1m,
        max_effort: None,
        tier: Some(tier.to_string()),
    }
}

#[test]
fn rows_carry_model_label_and_behaves_as() {
    let value = model_picker_value(&[route("claude-fable-1", "fable", Some("Zhipu GLM · glm-5.3"), false)]);
    assert_eq!(value["replaceBuiltInOptions"], json!(false));
    assert_eq!(
        value["options"],
        json!([{
            "model": "claude-fable-1",
            "label": "Zhipu GLM · glm-5.3",
            "behavesAs": "claude-fable-5"
        }])
    );
}

#[test]
fn appends_1m_row_only_when_supported() {
    let with = model_picker_value(&[route("claude-fable-1", "fable", Some("A"), true)]);
    assert_eq!(with["options"].as_array().unwrap().len(), 2);
    assert_eq!(with["options"][1]["model"], json!("claude-fable-1[1m]"));
    assert_eq!(with["options"][1]["label"], json!("A · 1M"));
    let without = model_picker_value(&[route("claude-fable-1", "fable", Some("A"), false)]);
    assert_eq!(without["options"].as_array().unwrap().len(), 1);
}

#[test]
fn omits_label_when_missing() {
    let value = model_picker_value(&[route("claude-fable-1", "fable", None, true)]);
    assert!(value["options"][0].get("label").is_none());
    assert!(value["options"][1].get("label").is_none()); // 无基名 → 1M 行同样省略（默认渲染 model 名自带 [1m]）
}

#[test]
fn maps_each_tier_to_its_known_id() {
    for (tier, known) in [
        ("fable", "claude-fable-5"),
        ("opus", "claude-opus-4-8"),
        ("sonnet", "claude-sonnet-4-6"),
        ("haiku", "claude-haiku-4-5"),
    ] {
        let value = model_picker_value(&[route("claude-x-1", tier, None, false)]);
        assert_eq!(value["options"][0]["behavesAs"], json!(known), "tier {tier}");
    }
}

#[test]
fn preserves_route_order() {
    let value = model_picker_value(&[
        route("claude-opus-4-8", "opus", Some("A"), false),
        route("claude-fable-1", "fable", Some("B"), true),
        route("claude-sonnet-4", "sonnet", Some("C"), false),
    ]);
    let models: Vec<_> = value["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["model"].as_str().unwrap())
        .collect();
    assert_eq!(models, ["claude-opus-4-8", "claude-fable-1", "claude-fable-1[1m]", "claude-sonnet-4"]);
}

#[test]
fn sync_writes_key_and_preserves_neighbors() {
    let db = crate::database::Database::memory().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","env":{"A":"b"}}"#).unwrap();
    sync_cli_model_picker_at(&db, &path, Some(&[route("claude-fable-1", "fable", Some("X"), false)])).unwrap();
    let saved: serde_json::Value = read_json_file(&path).unwrap();
    assert_eq!(saved["theme"], json!("dark"));
    assert_eq!(saved["env"], json!({"A":"b"}));
    assert_eq!(saved["modelPicker"]["options"][0]["model"], json!("claude-fable-1"));
}

#[test]
fn sync_removes_key_when_snapshot_matches() {
    let db = crate::database::Database::memory().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let routes = [route("claude-fable-1", "fable", Some("X"), false)];
    sync_cli_model_picker_at(&db, &path, Some(&routes)).unwrap();
    sync_cli_model_picker_at(&db, &path, None).unwrap();
    let saved: serde_json::Value = read_json_file(&path).unwrap();
    assert!(saved.get("modelPicker").is_none());
}

#[test]
fn sync_keeps_handwritten_picker() {
    let db = crate::database::Database::memory().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"modelPicker":{"options":[{"model":"claude-opus-5"}]}}"#).unwrap();
    sync_cli_model_picker_at(&db, &path, None).unwrap(); // 无存证 → 不动手写值
    let saved: serde_json::Value = read_json_file(&path).unwrap();
    assert_eq!(saved["modelPicker"]["options"][0]["model"], json!("claude-opus-5"));
}

#[test]
fn sync_reapply_overwrites_previous_picker() {
    let db = crate::database::Database::memory().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    sync_cli_model_picker_at(&db, &path, Some(&[route("claude-fable-1", "fable", Some("X"), false)])).unwrap();
    sync_cli_model_picker_at(&db, &path, Some(&[route("claude-opus-4-8", "opus", Some("Y"), false)])).unwrap();
    let saved: serde_json::Value = read_json_file(&path).unwrap();
    let models: Vec<_> = saved["modelPicker"]["options"].as_array().unwrap()
        .iter().map(|r| r["model"].as_str().unwrap()).collect();
    assert_eq!(models, ["claude-opus-4-8"]);
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `export PATH="/c/Users/Jason/.cargo/bin:$PATH" && cd src-tauri && cargo test --release model_picker`
Expected: 编译失败（`mod model_picker` 未声明 / 函数未定义）——先补 `lib.rs` 的 `mod model_picker;` 与桩函数，再跑至「红」（todo!() panic 或断言失败）。

- [ ] **Step 3: 最小实现**

```rust
// src-tauri/src/model_picker.rs 完整实现
//! Claude Code CLI `/model` 选择器的 `modelPicker` 同步（B：CLI 侧聚合）。
//! 设计：docs/superpowers/specs/2026-10-01-cli-model-picker-design.md

use std::path::Path;

use serde_json::{json, Value};

use crate::claude_desktop_config::ResolvedModelRoute;
use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
use crate::database::Database;
use crate::error::AppError;

/// 存证最近一次写入的 modelPicker 生成值。非聚合应用删键前与它比对，
/// 不一致（用户手写/改过）则不动——settings.json 是用户领地，不误删。
const GENERATED_SNAPSHOT_KEY: &str = "cli_model_picker_generated";

/// 档位 → 本版 CLI 认识的模型 ID（behavesAs）。取证见设计 D4。
const BEHAVES_AS: [(&str, &str); 4] = [
    ("fable", "claude-fable-5"),
    ("opus", "claude-opus-4-8"),
    ("sonnet", "claude-sonnet-4-6"),
    ("haiku", "claude-haiku-4-5"),
];

fn behaves_as_for(tier: Option<&str>) -> Option<&'static str> {
    tier.and_then(|t| {
        BEHAVES_AS
            .iter()
            .find(|(key, _)| *key == t)
            .map(|(_, id)| *id)
    })
}

fn make_row(model: &str, label: Option<&str>, behaves: Option<&str>) -> Value {
    let mut row = json!({ "model": model });
    if let Some(label) = label {
        row["label"] = Value::String(label.to_string());
    }
    if let Some(behaves) = behaves {
        row["behavesAs"] = Value::String(behaves.to_string());
    }
    row
}

/// 由路由表生成 modelPicker 值。顺序 = 路由序（供应商分组 × 档位、默认模型置顶），
/// supports_1m 的槽位在本体行后追加 `[1m]` 行。
pub fn model_picker_value(routes: &[ResolvedModelRoute]) -> Value {
    let mut options: Vec<Value> = Vec::with_capacity(routes.len() * 2);
    for route in routes {
        let behaves = behaves_as_for(route.tier.as_deref());
        let label = route.label_override.clone();
        options.push(make_row(&route.route_id, label.as_deref(), behaves));
        if route.supports_1m {
            let label_1m = label.as_ref().map(|l| format!("{l} · 1M"));
            options.push(make_row(
                &format!("{}[1m]", route.route_id),
                label_1m.as_deref(),
                behaves,
            ));
        }
    }
    json!({ "replaceBuiltInOptions": false, "options": options })
}

pub(crate) fn sync_cli_model_picker(
    db: &Database,
    routes: Option<&[ResolvedModelRoute]>,
) -> Result<(), AppError> {
    sync_cli_model_picker_at(db, &get_claude_settings_path(), routes)
}

/// `Some(routes)`：写入生成值并存证。`None`：仅当文件当前值与存证一致才删键。
pub(crate) fn sync_cli_model_picker_at(
    db: &Database,
    path: &Path,
    routes: Option<&[ResolvedModelRoute]>,
) -> Result<(), AppError> {
    match routes {
        Some(routes) => {
            let value = model_picker_value(routes);
            set_top_level_key(path, "modelPicker", Some(&value))?;
            db.set_setting(GENERATED_SNAPSHOT_KEY, &value.to_string())
        }
        None => {
            let stored = db
                .get_setting(GENERATED_SNAPSHOT_KEY)?
                .filter(|s| !s.is_empty())
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            if let Some(ours) = stored {
                let current = read_json_or_empty(path);
                if current.get("modelPicker") == Some(&ours) {
                    set_top_level_key(path, "modelPicker", None)?;
                }
                db.set_setting(GENERATED_SNAPSHOT_KEY, "")?;
            }
            Ok(())
        }
    }
}

fn read_json_or_empty(path: &Path) -> Value {
    if !path.exists() {
        return json!({});
    }
    read_json_file::<Value>(path).unwrap_or_else(|_| json!({}))
}

fn set_top_level_key(path: &Path, key: &str, value: Option<&Value>) -> Result<(), AppError> {
    if !path.exists() && value.is_none() {
        return Ok(());
    }
    let mut doc = read_json_or_empty(path);
    if !doc.is_object() {
        if value.is_none() || (doc.is_null() && !path.exists()) {
            return Ok(());
        }
        if !doc.is_null() {
            return Err(AppError::Config(format!(
                "{} 不是 JSON 对象，拒绝写入 {key}",
                path.display()
            )));
        }
        doc = json!({});
    }
    match value {
        Some(v) => doc[key] = v.clone(),
        None => {
            doc.as_object_mut().map(|obj| obj.remove(key));
        }
    }
    write_json_file(path, &doc)
}
```

（`AppError::Config` 变体名以 `src-tauri/src/error.rs` 实际为准；若无 `Config(String)` 就用 `AppError::Message(format!(...))`，两者先查再用。）

- [ ] **Step 4: 跑测试确认通过**

Run: `export PATH="/c/Users/Jason/.cargo/bin:$PATH" && cd src-tauri && cargo test --release model_picker`
Expected: 9 passed / 0 failed。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/model_picker.rs src-tauri/src/lib.rs
git commit -m "feat: generate CLI /model picker modelPicker from aggregate slots"
```

---

### Task 2: 接线到 `apply_provider`（与 inferenceModels 同寿命）

**Files:**
- Modify: `src-tauri/src/claude_desktop_config.rs`
  - `apply_provider`（L137-140）
  - `apply_provider_to_paths`（L996-1009）+ `with_rollback`（L1015-1032，签名泛型化）
  - `apply_provider_to_paths_inner`（L1034 起，返回值带上路由）
- Test: 同文件 `mod tests`（沿用 TempHome + `#[serial]` 惯例）

**Interfaces:**
- Consumes: Task 1 的 `model_picker::sync_cli_model_picker`。
- Produces: `apply_provider_to_paths` 返回 `Result<Option<Vec<ResolvedModelRoute>>, AppError>`——聚合/代理映射 `Some(routes)`，直连与官方 `None`。现有测试调用点用 `.expect(...)`/`.expect_err(...)` 丢弃返回值，**无需修改**（`Option<Vec<_>>` 非 `#[must_use]`）。

- [ ] **Step 1: 写失败测试**

```rust
// claude_desktop_config.rs mod tests 内新增（构造器沿用该模块既有 helper）
#[test]
fn apply_provider_to_paths_returns_routes_for_aggregate_and_none_for_plain() {
    let db = test_db();                       // 沿用模块内既有 helper
    let paths = test_paths(&TempHome::new().path().to_path_buf()); // 以模块既有写法为准
    // 聚合：Some(routes)，route_id 与槽位一致
    // 直连（非聚合）：None；官方：None
}

#[test]
#[serial]
fn apply_provider_syncs_model_picker_and_removes_it_for_plain_provider() {
    // TempHome 下：
    // 1) apply_provider(聚合) → <home>/.claude/settings.json 出现 modelPicker，
    //    options[i].model == 槽位 route_id，replaceBuiltInOptions == false
    // 2) apply_provider(直连普通) → modelPicker 键消失，邻键保留
    // 3) 再手写一个 modelPicker 后 apply_provider(直连) → 手写值保留
}
```

（断言值按 Task 1 测试同款形状；实现者以模块内 `TempHome`/`#[serial]`/`test_db` 的实际名字为准——`services/provider/mod.rs:137` 与本模块 tests 均有样板。）

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test --release claude_desktop_config::tests::apply_provider`
Expected: 编译失败（返回类型未改）或断言失败（尚未接线）。

- [ ] **Step 3: 实现**

1. `with_rollback` 泛型化（返回被包裹闭包的值）：

```rust
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
```

2. `apply_provider_to_paths` / `_inner` 返回 `Result<Option<Vec<ResolvedModelRoute>>, AppError>`：
   - 官方分支：`restore_official_at_paths(paths)?; return Ok(None);`
   - Direct 分支：构建 profile 后 `Ok(None)`（B 不为直连生成行）
   - Proxy 分支：`let routes = proxy_model_routes(provider)?;` 既有 map 到 model_specs 不变，
     返回值**仅当 `crate::aggregate::is_aggregate_provider(provider)` 时** `Some(routes)`，
     普通代理供应商（模型映射行）同样返回 `None`（spec 范围：只覆盖聚合）

3. `apply_provider` 接线（衍生件失败降级 warn，spec D5）：

```rust
pub fn apply_provider(db: &Database, provider: &Provider) -> Result<(), AppError> {
    let paths = current_platform_paths()?;
    let routes = apply_provider_to_paths(db, provider, &paths)?;
    if let Err(err) = crate::model_picker::sync_cli_model_picker(db, routes.as_deref()) {
        log::warn!("modelPicker 同步失败（下次 apply 自愈）: {err}");
    }
    Ok(())
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test --release claude_desktop_config`
Expected: 新增 2 条通过；既有 claude_desktop 测试**零回归**（唯一已知失败为环境性端口占用那条，若出现需与基线比对名单而非计数）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/claude_desktop_config.rs
git commit -m "feat: sync CLI modelPicker alongside Desktop profile on apply"
```

---

### Task 3: 全量验证

**Files:** 无新改动（若失败回到 Task 1/2 修）

- [ ] **Step 1: fmt**

Run: `cd src-tauri && cargo fmt --check`
Expected: exit 0、零输出。

- [ ] **Step 2: 全量测试**

Run: `export PATH="/c/Users/Jason/.cargo/bin:$PATH" && cd src-tauri && cargo test --release --lib --no-fail-fast`
Expected: 失败项 ⊆ 既有 10 条环境性失败名单（model_pricing×5、import_hermes、端口 10048、symlink×2、commands::misc；对照账本 2026-09-26 名单）；**零新增失败**；真实 profile/settings.json md5 前后不变。

- [ ] **Step 3: Commit（如有修动）**

---

### Task 4: 构建、部署与验收

**Files:** 无代码；产物与账本

- [ ] **Step 1: 构建**（UPSTREAM-SYNC §5：`export PATH`；NSIS 走 gh-proxy 镜像 + `unset *_PROXY`）

Run: `pnpm tauri build --bundles nsis`
Expected: `src-tauri/target/release/bundle/nsis/CC Switch_3.20.4-local_x64-setup.exe`；记录 md5、前端资源名。

- [ ] **Step 2: 部署**（UPSTREAM-SYNC §6：先轮询确认旧进程退出，再 `tools/install-local.bat`）
Expected: 安装目录 exe md5 与产物一致；官方 pubkey 计数 0。

- [ ] **Step 3: 验收**（能自动的先自动）
1. 自动：写一条脚本断言 `~/.claude/settings.json` 的 modelPicker 与 DB 聚合槽位一致（route_id 集合、1M 行、behavesAs）——重放 apply 或经 UI 切换一次聚合供应商后读文件比对。
2. 手动（留用户）：重启 Claude Code CLI（Desktop Code 面板 / 终端）→ `/model` 选择器出现聚合行（「供应商 · 模型」标签、1M 行）→ 选中任一槽位发消息 → `proxy_request_logs` 归属正确上游。

- [ ] **Step 4: 账本与快照**
progress.md 追加 B 任务记录；工作区文档如有改动刷新 `docs/local-maintenance/` 快照。
