# Codex 聚合实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 一个 Codex 聚合供应商（无端点无凭据）按客户端模型名把 `/v1/responses` 分流到多家 Codex 供应商，且槽位模型出现在 Codex CLI 的模型选择器（catalog 文件）。

**Architecture:** 独立 meta 键 `codexAggregateRoutes`（避开上游 #5937 的 `aggregateRoutes` 碰撞）；数据面复用 `aggregate_override` 管道（开 AppType 双门 + 改写点前移一次覆盖原生透传/Chat/Anthropic 三形态）；控制面在 live 写入的有效配置构建处合成 seed TOML + auth 占位 + `modelCatalog`，走既有 catalog 文件管线落盘。

**Tech Stack:** Rust（serde/rusqlite/axum）、React+TS（vitest）。

**Spec:** `docs/superpowers/specs/2026-10-01-codex-aggregate-design.md`（决策 D1–D6，用户已批准）· 事实基础：`.superpowers/sdd/codex-aggregate-research.md`

## Global Constraints

- 测试绝不写真实用户文件（`~/.codex/`、`~/.claude/`、真实 3P profile）：Rust 测试用 TempHome（pin `CC_SWITCH_TEST_HOME`/`HOME`/`USERPROFILE`/`LOCALAPPDATA`，Linux 另 pin `XDG_CONFIG_HOME`）+ `#[serial]`，或 `_at` 显式路径缝。2026-09-22 事故教训。
- Rust 测试一律 `export PATH="/c/Users/Jason/.cargo/bin:$PATH"` + `cargo test --release ...`（bare `cargo test` 生成 18G debug 树，禁用）。前端 `npx vitest run <file>`（`pnpm test:unit --` 不过滤）；`pnpm typecheck` 过。
- 注释仅写「约束/为什么」，与所在文件语言风格一致；`cargo fmt --check` 0 diff。
- 键名逐字：serde `codexAggregateRoutes`、seed provider id `cc-switch-aggregate`、auth 占位 `OPENAI_API_KEY = "PROXY_MANAGED"`、`wire_api = "responses"`。
- Claude 侧的 claude-safe 约束、tier、behavesAs 均**不**适用于 Codex 槽位（`model` 为自由字符串）。
- 全表槽位 `model` 非空且唯一（保存校验兜底，运行时 resolve 首匹配）。

---

### Task 1: 数据模型 `CodexAggregateRoutes`

**Files:**
- Modify: `src-tauri/src/aggregate.rs`（文件尾部追加，紧邻 `AggregateRoutes` 家族）
- Modify: `src-tauri/src/provider.rs`（`ProviderMeta` 的 `aggregate_routes` 邻字段处，约 :468）
- Test: 同文件 `mod tests`（参照 `provider.rs` 中 `aggregate_routes` 的既有往返测试形态）

**Interfaces:**
- Consumes: 既有 `DefaultTarget`（`aggregate.rs`，tagged 枚举 `ProviderId(String) | SlotId(String)`）、`serde::{Serialize, Deserialize}`。
- Produces（后续任务依赖，签名逐字）:
  - `pub struct CodexAggregateRoutes { pub slots: Vec<CodexAggregateSlot>, pub default_target: DefaultTarget, pub default_model: Option<String> }`
  - `pub struct CodexAggregateSlot { pub model: String, pub provider_id: String, pub upstream_model: String, pub label: Option<String> }`
  - `ProviderMeta.codex_aggregate_routes: Option<CodexAggregateRoutes>`（serde `codexAggregateRoutes`，`default` + `skip_serializing_if = "Option::is_none"`）
  - `pub fn is_codex_aggregate_provider(provider: &Provider) -> bool`
  - `pub(crate) fn codex_routes_of(provider: &Provider) -> Result<&CodexAggregateRoutes, AppError>`

- [ ] **Step 1: 写失败测试**（`provider.rs` tests + `aggregate.rs` tests）

```rust
#[test]
fn codex_aggregate_routes_round_trip() {
    // 反序列化 camelCase JSON → 结构 → 再序列化，字段逐一断言
    let json = serde_json::json!({
        "codexAggregateRoutes": {
            "slots": [
                {"model": "gpt-5.1", "providerId": "p-kimi", "upstreamModel": "kimi-k2"},
                {"model": "glm-5.3", "providerId": "p-zhipu", "upstreamModel": "glm-5.3", "label": "Zhipu · glm-5.3"}
            ],
            "defaultTarget": {"kind": "providerId", "value": "p-kimi"},
            "defaultModel": "gpt-5.1"
        }
    });
    let meta: ProviderMeta = serde_json::from_value(json).expect("deserialize");
    let routes = meta.codex_aggregate_routes.expect("present");
    assert_eq!(routes.slots.len(), 2);
    assert_eq!(routes.slots[0].model, "gpt-5.1");
    assert_eq!(routes.slots[1].label.as_deref(), Some("Zhipu · glm-5.3"));
    assert_eq!(routes.default_target, DefaultTarget::ProviderId("p-kimi".into()));
    assert_eq!(routes.default_model.as_deref(), Some("gpt-5.1"));
    let out = serde_json::to_value(&meta).expect("serialize");
    assert!(out.get("codexAggregateRoutes").is_some());
}

#[test]
fn codex_aggregate_routes_absent_when_unset() {
    let meta: ProviderMeta = serde_json::from_value(serde_json::json!({})).expect("deserialize");
    assert!(meta.codex_aggregate_routes.is_none());
    let out = serde_json::to_value(&meta).expect("serialize");
    assert!(out.get("codexAggregateRoutes").is_none()); // skip_serializing_if
}

#[test]
fn codex_routes_of_errors_when_missing() { /* is_codex_aggregate_provider false + codex_routes_of Err */ }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test --release --lib codex_aggregate`
Expected: 编译失败（字段/类型未定义）。

- [ ] **Step 3: 实现**

```rust
// aggregate.rs —— 紧邻 AggregateRoutes 定义之后
/// Codex 聚合路由：客户端模型名（精确匹配）→ 目标 Codex 供应商 + 上游模型。
/// 与 Claude 侧 `AggregateRoutes` 平行、互不兼容（上游 PR #5937 占用 `aggregateRoutes`
/// 键，此处用独立键名避开碰撞）。数据面见 `resolve_codex_target`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAggregateRoutes {
    pub slots: Vec<CodexAggregateSlot>,
    pub default_target: DefaultTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAggregateSlot {
    /// 客户端模型名（slug，路由键）。非空与全表唯一由保存校验兜底。
    pub model: String,
    pub provider_id: String,
    pub upstream_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

pub fn is_codex_aggregate_provider(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.codex_aggregate_routes.as_ref())
        .is_some()
}

pub(crate) fn codex_routes_of(provider: &Provider) -> Result<&CodexAggregateRoutes, AppError> {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.codex_aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "codex_aggregate.routes_missing",
                "Codex 聚合供应商缺少路由表",
                "Codex aggregate provider is missing its route table",
            )
        })
}
```

```rust
// provider.rs —— ProviderMeta 内、aggregate_routes 邻字段（同风格）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_aggregate_routes: Option<crate::aggregate::CodexAggregateRoutes>,
```

（`ProviderMeta` 现有 100+ 处 `..Default::default()` 构造不受影响；确认 `Default` 派生覆盖新字段。）

- [ ] **Step 4: 绿灯 + fmt**

Run: `cargo test --release --lib codex_aggregate`（3 passed）；`cargo fmt --check` 0。
Run（回归）: `cargo test --release --lib provider::tests`（既有 serde 测试零回归）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/aggregate.rs src-tauri/src/provider.rs
git commit -m "feat: codex aggregate route table data model"
```

---

### Task 2: 请求路由（AppType 双门 + 防二次改写 + 端到端）

**Files:**
- Modify: `src-tauri/src/aggregate.rs`（新增 `resolve_codex_target`）
- Modify: `src-tauri/src/proxy/handler_context.rs`（:156-195 聚合分支）
- Modify: `src-tauri/src/proxy/forwarder.rs`（:1312 附近 override 门 + :1645/:1662 附近两个 `apply_codex_*_upstream_model` 调用点）
- Test: `aggregate.rs` tests + `proxy/server.rs` tests（e2e，参照 :620-740 的 axum mock 上游夹具）

**Interfaces:**
- Consumes: Task 1 全部；既有 `strip_one_m_suffix_for_route_lookup`（`claude_desktop_config.rs`，已 `pub(crate)`）、`RequestForwarder.aggregate_override` 管道、`DefaultTarget`。
- Produces:
  - `pub fn resolve_codex_target(db: &Database, app_type_str: &str, aggregate: &Provider, request_model: &str) -> Result<(Provider, Option<String>), AppError>`

- [ ] **Step 1: 写失败测试**

```rust
// aggregate.rs tests
#[tokio::test]
async fn resolve_codex_target_exact_match_rewrites_model() {
    // 槽位 {"model":"gpt-5.1","providerId":"p-kimi","upstreamModel":"kimi-k2"} 命中
    // → (p-kimi, Some("kimi-k2"))
}
#[tokio::test]
async fn resolve_codex_target_falls_back_to_default_provider_without_rewrite() {
    // 未命中 + defaultTarget=ProviderId("p-other") → (p-other, None)
}
#[tokio::test]
async fn resolve_codex_target_default_slot_rewrites_to_slot_upstream() {
    // 未命中 + defaultTarget=SlotId("gpt-5.1") → (p-kimi, Some("kimi-k2"))  // 与 Claude 侧同款：SlotId 兜底保留该槽上游名
}
#[tokio::test]
async fn resolve_codex_target_dangling_default_slot_errors() {
    // defaultTarget=SlotId("missing") → Err(localized_key == "codex_aggregate.default_target_slot_missing")
}
#[tokio::test]
async fn resolve_codex_target_strips_1m_marker_before_lookup() {
    // 请求 "gpt-5.1[1M]"（或 [1m]）→ 命中 "gpt-5.1" 槽位，不得漏到兜底
}
```

```rust
// proxy/server.rs tests —— e2e（真 ProxyServer + axum mock 上游 + Database::memory）
#[tokio::test]
async fn codex_aggregate_native_passthrough_rewrites_model_and_credentials() {
    // 当前供应商 = codex 聚合（槽位 model "gpt-5.1" → p-target + upstreamModel "kimi-k2"）
    // POST /v1/responses {"model":"gpt-5.1", ...}
    // 断言 mock 收到 body.model == "kimi-k2"  ← 原生透传路径的改写（本任务核心）
    // 断言 Authorization == 目标供应商凭据
    // 断言 db.get_current_provider("codex") 不被切走、failover_count == 0
}
#[tokio::test]
async fn codex_aggregate_skips_apply_codex_upstream_model_when_overridden() {
    // 目标供应商带 modelCatalog（含 "kimi-k2"）与顶层 model="other"
    // 断言出站 model 仍是 "kimi-k2"（不被 catalog 白名单/默认模型二次改写）
}
```

- [ ] **Step 2: 红灯**

Run: `cargo test --release --lib resolve_codex_target`（编译失败/断言失败）。

- [ ] **Step 3: 实现**

`resolve_codex_target`（aggregate.rs，紧邻 `resolve_target`）：查找前 `strip_one_m_suffix_for_route_lookup(request_model)`，然后与 Claude 侧同构三层：槽位 `model` **精确匹配**（无别名层）→ `default_target`（`ProviderId` → `(provider, None)`；`SlotId` → 该槽 `(provider_id, Some(upstream_model))`，悬空 → `codex_aggregate.default_target_slot_missing`）；目标供应商缺失 → 明确报错（同 `resolve_target` 的 `load_provider` 错误路径）。

`handler_context.rs:156-195`：聚合分支改 match：

```rust
let aggregate_override = match app_type {
    AppType::ClaudeDesktop if crate::aggregate::is_aggregate_provider(&provider) => {
        let (target, upstream) =
            crate::aggregate::resolve_target(&state.db, app_type_str, &provider, &request_model)
                .map_err(|e| ProxyError::ConfigError(e.to_string()))?;
        // ……既有日志 + providers[0]=target + 链截断，原样保留……
        Some(...)
    }
    AppType::Codex if crate::aggregate::is_codex_aggregate_provider(&provider) => {
        let (target, upstream) = crate::aggregate::resolve_codex_target(
            &state.db, app_type_str, &provider, &request_model,
        )
        .map_err(|e| ProxyError::ConfigError(e.to_string()))?;
        // 同上：日志 + providers[0]=target + truncate(1) + Some(AggregateOverride{..})
        Some(...)
    }
    _ => None,
};
```

（两分支尾部的「换 providers[0] + 截断 + 注入」逻辑抽私有 helper 复用，避免逐字重复；注释同步更新「Claude Desktop 专属」表述为两 AppType。）

`forwarder.rs:1312` 附近：override 改写门放宽——`self.aggregate_override` 存在即改写 `body.model`（原本仅 ClaudeDesktop 分支消费；Codex 请求 body 同为顶层 `model` 字段，无需新适配）。**:1645/:1662 两个 `apply_codex_*_upstream_model` 调用点**加守卫：

```rust
if self.aggregate_override.is_none() {
    apply_codex_chat_upstream_model(provider, &mut mapped_body);
}
```

（catalog 白名单会在目标 catalog 撞名时吞掉/误改聚合改写结果——聚合层拥有模型名语义，research §5.5。）

- [ ] **Step 4: 绿灯 + 回归**

Run: `cargo test --release --lib resolve_codex_target` + `cargo test --release --lib aggregate::tests` + `cargo test --release --lib proxy::`（既有 forwarder/handler 测试零回归；Mimo 旁路等既有语义不许变化）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/aggregate.rs src-tauri/src/proxy/handler_context.rs src-tauri/src/proxy/forwarder.rs
git commit -m "feat: route codex aggregate requests by client model name"
```

---

### Task 3: 控制面合成（seed TOML + auth 占位 + modelCatalog）

**Files:**
- Modify: `src-tauri/src/services/provider/live.rs`（`build_effective_provider_for_live_with_codex_oauth_manager`，:872-884）
- Create 或 Modify: 合成函数放 `src-tauri/src/aggregate.rs`（数据→配置的派生逻辑，与 `aggregate_model_routes` 同居；**控制器裁决**：放 aggregate.rs 而非 codex_config.rs——它只消费槽位表、不触碰 codex 文件 IO）
- Test: `aggregate.rs` tests + `live.rs` tests（TempHome）

**Interfaces:**
- Consumes: Task 1；`crate::config` 的 proxy origin 派生（`proxy_origin_from_parts` 或同效函数；Codex seed 的 `base_url` = origin **根路径**，无 `/claude-desktop` 前缀——`/v1/responses` 挂在根路由）。
- Produces:
  - `pub fn synthesize_codex_aggregate_settings(routes: &CodexAggregateRoutes, proxy_origin: &str) -> Result<serde_json::Value, AppError>`（返回 `{auth, config, modelCatalog}` 三键的 settings_config）
  - 派生 helper `pub fn codex_aggregate_model_catalog(routes: &CodexAggregateRoutes) -> Vec<serde_json::Value>`（`[{model, displayName?}]`，default_model 命中置顶）

- [ ] **Step 1: 失败测试**

```rust
#[test]
fn codex_aggregate_seed_toml_places_fields_correctly() {
    let settings = synthesize_codex_aggregate_settings(&routes, "http://127.0.0.1:15721").unwrap();
    let config = settings["config"].as_str().unwrap();
    let header = config.find("[model_providers.cc-switch-aggregate]").unwrap();
    // 顶层 model 在表头之前
    let model_line = config.find("model = \"gpt-5.1\"").unwrap();
    assert!(model_line < header);
    // base_url / wire_api 在表头之后（表内）
    assert!(config.find("base_url = \"http://127.0.0.1:15721\"").unwrap() > header);
    assert!(config.find("wire_api = \"responses\"").unwrap() > header);
    assert!(config.contains("model_provider = \"cc-switch-aggregate\""));
    assert!(config.contains("requires_openai_auth = true"));
    assert!(config.contains("disable_response_storage = true"));
    assert_eq!(settings["auth"]["OPENAI_API_KEY"], json!("PROXY_MANAGED"));
}
#[test]
fn codex_aggregate_catalog_preserves_slot_order_and_pins_default() {
    // 3 槽 + defaultModel=第 3 槽 → catalog[0].model == 第 3 槽，其后 1、2 槽
    // label 非空 → displayName；label 空 → 无 displayName 键
}
#[test]
fn apply_provider_effective_settings_synthesize_for_aggregate() {
    // TempHome + #[serial]：对聚合供应商走 write_live_with_common_config_for_codex_oauth_manager
    // 后，~/.codex/config.toml 含 seed 结构、cc-switch-model-catalog.json 的 slug 集合 == 槽位 model 集合
}
```

- [ ] **Step 2: 红灯** → Run: `cargo test --release --lib codex_aggregate`（编译失败）。

- [ ] **Step 3: 实现**

`synthesize_codex_aggregate_settings`：default_model（`trim` 非空且命中槽位）置顶 catalog 并取作顶层 `model`；否则 `slots[0].model`。`config` 用 `format!` 生成（seed 结构逐字见上；`{origin}` 来自入参）。槽位 `model`/`upstream`/`label` 为空的槽在派生 catalog 时跳过（上游模型空 = 半成品规则，与 Claude 侧 `aggregate_model_routes` 的 trim+过滤同款）。

`live.rs` 接线（`build_effective_provider_for_live_with_codex_oauth_manager` 内、`apply_codex_official_auth` 之前）：

```rust
if matches!(app_type, AppType::Codex)
    && crate::aggregate::is_codex_aggregate_provider(&effective_provider)
{
    let routes = crate::aggregate::codex_routes_of(&effective_provider)?.clone();
    let origin = /* proxy origin 根：同 claude_desktop_config.rs:985-993 的派生、去尾缀 */;
    effective_provider.settings_config =
        crate::aggregate::synthesize_codex_aggregate_settings(&routes, &origin)?;
}
```

（合成必须发生在 `neutralize_codex_proxy_oauth_fallback` 之前；`preflight_codex_live_write_for_state` 共用此 builder，自动获得同样语义——「预检与写入一致」的既有约束不破坏。）

- [ ] **Step 4: 绿灯 + 回归** → `cargo test --release --lib codex_aggregate` + `cargo test --release --lib services::provider`（sync_codex_live 校验路径零回归）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/aggregate.rs src-tauri/src/services/provider/live.rs
git commit -m "feat: synthesize codex aggregate live config and model catalog"
```

---

### Task 4: 保存校验与删除保护泛化

**Files:**
- Modify: `src-tauri/src/services/provider/mod.rs`（`validate_aggregate_not_nested`、槽位校验、`reject_if_referenced_by_aggregate`——先 grep 既有函数名与落点，按函数内定位插入）
- Test: 同模块 tests（Localized key 断言，参照既有 `validate_aggregate_*` 测试形态）

**Interfaces:**
- Consumes: Task 1。
- Produces: 校验在 add/update 路径自动生效（既有 6 处 `validate_provider_settings` 调用点无需改签名）；错误 key：`codex_aggregate.slot_model_empty`、`codex_aggregate.slot_model_duplicate`、`codex_aggregate.slot_provider_empty`、`codex_aggregate.slot_upstream_empty`、`codex_aggregate.nested`、`codex_aggregate.default_target_slot_missing`。

- [ ] **Step 1: 失败测试**（每个 key 一条正例 + 一条「合法表通过」对照；删除保护：建聚合→建槽位目标→删目标被拒；反向：目标被引用后不得被改造成聚合）

- [ ] **Step 2: 红灯** → `cargo test --release --lib services::provider`

- [ ] **Step 3: 实现**：槽位校验规则=「model 非空、全表 trim 后唯一、provider_id/upstream_model 非空」；禁嵌套=目标不得是 `is_codex_aggregate_provider`（Claude/Codex 两方向都查，两个键互不引用）；删除保护 helper 泛化为「两个键任一引用即拒」。defaultTarget 校验与 Task 2 的运行时语义对齐（空 `providerId`/悬空 `slotId` 保存时拦截——设计 §D5 要求，与 Claude 侧 #7786 同款）。

- [ ] **Step 4: 绿灯 + 回归** → `cargo test --release --lib services::provider`（既有 aggregate 校验测试零回归）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/provider/mod.rs
git commit -m "feat: validate codex aggregate routes and protect their targets"
```

---

### Task 5: 前端（类型 + 扁平行编辑器 + i18n）

**Files:**
- Modify: `src/types.ts`（`ProviderMeta` 增 `codexAggregateRoutes?`；新增 `CodexAggregateRoutes`/`CodexAggregateSlot` 类型，camelCase 与后端 serde 对齐）
- Create: `src/components/providers/forms/CodexAggregateFields.tsx`（扁平行编辑器）
- Modify: `src/components/providers/forms/CodexFormFields.tsx`（`!isOfficial` 顶部聚合开关接线；开启初始化 `{slots:[], defaultTarget}`、关闭 `delete meta.codexAggregateRoutes`）
- Modify: `src/i18n/locales/zh.json` + `en.json`（`codexAggregate.*` 键：title/enable/model/provider/upstreamModel/label/addSlot/removeSlot/defaultTarget/defaultModel/notSaveable/modeLockedHint 等，与 Claude 侧 `aggregate.*` 同名风格）
- Test: `tests/components/CodexAggregateFields.test.tsx` + 类型提交形状断言

**Interfaces:**
- Consumes: `fetchModelsForConfig`（`src/lib/api/model-fetch.ts`，「获取模型列表」按**供应商 id** 缓存——同 Claude 侧聚合的教训：别按槽位索引缓存）。
- Produces: 提交时 `meta.codexAggregateRoutes` 形状与 Task 1 的 serde 完全一致（`slots[{model,providerId,upstreamModel,label?}]`、`defaultTarget{kind,value}`、`defaultModel?`）。

- [ ] **Step 1: 失败测试**（vitest：开关初始化/删键；行编辑提交形状；模型下拉缓存按 provider id；空 model 行的保存提示走后端 Localized toast——UI 不做硬门禁，与 Claude 侧一致）
- [ ] **Step 2: 红灯** → `npx vitest run tests/components/CodexAggregateFields.test.tsx`
- [ ] **Step 3: 实现**（行为契约：每行 = 模型名 Input + 目标供应商 Select（排除自身与聚合）+ 上游模型 Input（右侧条件渲染 ModelDropdown + 「获取模型列表」按钮）+ 显示名 Input + 行尾删除；底部「+ 添加模型」；defaultModel/兜底目标两个 Select 与 Claude 侧同交互、文案用「兜底目标」；无档位行）
- [ ] **Step 4: 绿灯** → `npx vitest run tests/components/CodexAggregateFields.test.tsx` + `pnpm typecheck`
- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/components/providers/forms/CodexAggregateFields.tsx src/components/providers/forms/CodexFormFields.tsx src/i18n/locales/zh.json src/i18n/locales/en.json tests/components/CodexAggregateFields.test.tsx
git commit -m "feat: codex aggregate editor UI"
```

---

### Task 6: 全量验证

- [ ] `cargo fmt --check` → 0。
- [ ] `cargo test --release --lib --no-fail-fast` → 失败项 ⊆ 既有环境性名单（model_pricing×5 / import_hermes / update_current_claude_desktop(10048) / codex_config+session_usage_grokbuild(symlink) / commands::misc），**零新增**；真实 profile/settings.json/`~/.codex/*` md5 前后不变。
- [ ] `npx vitest run` 全量对照基线失败集（PiProviderForm/App 并发抖动看名单不看数字）+ `pnpm typecheck`。
- [ ] 如有修复 → 追加提交。

---

### Task 7: 构建、部署与验收

- [ ] 构建（UPSTREAM-SYNC §5：PATH export、`TAURI_BUNDLER_TOOLS_GITHUB_MIRROR`、`unset *_PROXY`）→ `pnpm tauri build --bundles nsis`；记录产物 md5 + 前端资源名。
- [ ] 部署（UPSTREAM-SYNC §6：`taskkill` 后**轮询确认退出**→ `tools/install-local.bat` → 重启）→ 三重校验：exe md5 双向一致、`grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6"` = 0、前端资源名命中 1。
- [ ] 自动验收：`grep -a -c "codexAggregateRoutes"`（或 `cc-switch-aggregate`）在安装 exe 中命中 ≥1（新代码进包）；对 Codex 聚合做一次 apply 后比对 `~/.codex/cc-switch-model-catalog.json` 的 slug 集合 == DB 槽位 model 集合、`config.toml` 含 seed 结构。
- [ ] 手动验收（留用户）：Codex CLI `/model` 出现槽位模型 → 选中逐槽发消息 → `proxy_request_logs` 归属正确（模型名 == 槽位 upstreamModel）；chat/anthropic 型目标各验一槽（转换路径）。
- [ ] 账本 + 文档快照收尾。
