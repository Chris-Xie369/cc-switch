# Claude Desktop 多供应商共存（聚合供应商）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Claude Desktop 在一次重启后，模型选择器里同时列出多家供应商的模型，用户可自由选用，无需回 CC Switch 切换并重启。

**Architecture:** 引入「聚合供应商」——一种无端点无凭据的虚拟供应商，路由表挂在 `Provider.meta.aggregateRoutes` 上。模型列表由槽位派生（复用既有的 `proxy_model_routes` / `model_list_response` 两条出口）；请求到达代理时按 `body.model` 查路由表，把「当前供应商」换成目标供应商、把模型名改写为槽位的上游模型，其余（认证/协议转换/熔断/故障转移）全部复用目标供应商的既有能力。

**Tech Stack:** Rust（Tauri v2 后端）、React + TypeScript（前端）、serde（序列化）、vitest（前端单测）。

**设计文档:** `docs/superpowers/specs/2026-09-22-claude-desktop-multi-provider-design.md`（唯一事实来源）

## Global Constraints

- 分支 `fix/profile-merge` 仅本地 + 推送到自己的 fork；**绝不推送到 upstream**。
- 只改本计划列出的文件；不新增 cargo/npm 依赖。
- 生成的槽位 ID 必须通过既有 `is_claude_safe_model_id`（形如 `claude-{sonnet|opus|haiku|fable}-{非空标识}`）；违规会导致 Claude Desktop **整组拒收**。
- Rust 注释用中文，与仓库既有风格一致；`AppError::localized(key, 中文, English)` 是错误构造的既有惯例。
- 提交信息**不得包含任何署名行**（如 Co-Authored-By）。
- 本机环境：`cargo` 不在默认 PATH，需 `export PATH="/c/Users/Jason/.cargo/bin:$PATH"`；cargo 命令在 `src-tauri/` 下执行。
- 跑完 `cargo test` 后清理 `src-tauri/target/debug`（15–24 G），或改用 `cargo test --release`。

---

### Task 1: 槽位数据模型与 ID 生成（纯逻辑）

**Files:**
- Create: `src-tauri/src/aggregate.rs`
- Modify: `src-tauri/src/lib.rs`（加 `mod aggregate;`）

**Interfaces:**
- Consumes: `crate::claude_desktop_config::is_claude_safe_model_id(&str) -> bool`（已存在）
- Produces: `AggregateTier`、`AggregateRouteSlot`、`AggregateRoutes`、`DefaultTarget`；`slugify(&str) -> String`；`generate_slot_id(tier, provider_name, provider_id, taken: &[String]) -> String`

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/aggregate.rs`，先只写测试与类型签名：

```rust
//! 聚合供应商：无端点无凭据的虚拟供应商，按模型把请求分流到其他供应商。

use serde::{Deserialize, Serialize};

/// 档位：决定 Claude Desktop 选择器里那句描述文字来自目录中哪个角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregateTier {
    Sonnet,
    Opus,
    Haiku,
    Fable,
}

impl AggregateTier {
    pub fn as_str(self) -> &'static str {
        match self {
            AggregateTier::Sonnet => "sonnet",
            AggregateTier::Opus => "opus",
            AggregateTier::Haiku => "haiku",
            AggregateTier::Fable => "fable",
        }
    }
}

/// 一个槽位 = 一个可被 Claude 选择的模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateRouteSlot {
    /// 生成的槽位 ID（**持久化**，是稳定的路由键）。
    /// 保存时由 `generate_slot_id` 依据目标供应商的名称生成并写入；
    /// 运行时直接用它查表，不再重新生成——这样重命名供应商不会改变已生效的路由。
    pub route_id: String,
    pub tier: AggregateTier,
    /// 目标供应商 id（被引用者受删除保护）
    pub provider_id: String,
    /// 该供应商的上游模型名
    pub upstream_model: String,
    /// 选择器显示名，如 "智谱 GLM-5.3"；缺省用上游模型名
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 勾选后该模型在选择器里会多出一行 "1M context window"
    #[serde(default)]
    pub supports_1m: bool,
}

/// 未命中路由时的兜底目标。按槽位 ID 或供应商 id 引用（不用下标——下标会随增删重排失效）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum DefaultTarget {
    SlotId(String),
    ProviderId(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateRoutes {
    pub slots: Vec<AggregateRouteSlot>,
    pub default_target: DefaultTarget,
}

/// 供应商名称 → slug。规则：**只保留 ASCII 字母数字**（其余字符视作分隔符并合并）→
/// 转小写 → 去首尾 '-' → 截断 20 字符。**仅当结果为空**（名称为纯非 ASCII，如 "月之暗面"）
/// 才回落为 provider_id 前 8 位。
/// 注意："智谱 GLM" 的 ASCII 部分是 GLM → slug 为 "glm"，**不会**回落。
pub fn slugify(provider_name: &str, provider_id: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in provider_name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let truncated = trimmed.chars().take(20).collect::<String>();
    let truncated = truncated.trim_matches('-').to_string();
    if truncated.is_empty() {
        provider_id.chars().take(8).collect()
    } else {
        truncated
    }
}

/// 生成槽位 ID：`claude-{tier}-{slug}`；与 taken 冲突时追加 `-2`、`-3`…
pub fn generate_slot_id(
    tier: AggregateTier,
    provider_name: &str,
    provider_id: &str,
    taken: &[String],
) -> String {
    let base = format!("claude-{}-{}", tier.as_str(), slugify(provider_name, provider_id));
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_desktop_config::is_claude_safe_model_id;

    #[test]
    fn slugify_ascii_name() {
        assert_eq!(slugify("DeepSeek-OTN", "abc12345"), "deepseek-otn");
        assert_eq!(slugify("OpenCode Go", "abc12345"), "opencode-go");
    }

    #[test]
    fn slugify_uses_ascii_part_of_mixed_name() {
        // "智谱 GLM" 的 ASCII 部分是 GLM：非 ASCII 字符被跳过（不产生分隔符），
        // 得到可读的 slug —— 不要误以为"含中文就回落"
        assert_eq!(slugify("智谱 GLM", "97a1d0df-b9a9"), "glm");
    }

    #[test]
    fn slugify_falls_back_to_provider_id_when_no_ascii() {
        // 名称为纯非 ASCII 时 slug 为空，才回落为 provider id 前 8 位
        assert_eq!(slugify("月之暗面", "97a1d0df-b9a9"), "97a1d0df");
    }

    #[test]
    fn slugify_truncates_and_cleans_edges() {
        assert_eq!(slugify("  ---A--B---  ", "x"), "a-b");
        assert_eq!(slugify("abcdefghijklmnopqrstuvwxyz", "x").len(), 20);
    }

    #[test]
    fn generate_slot_id_is_claude_safe() {
        let id = generate_slot_id(AggregateTier::Sonnet, "DeepSeek-OTN", "pid", &[]);
        assert_eq!(id, "claude-sonnet-deepseek-otn");
        assert!(is_claude_safe_model_id(&id));
    }

    #[test]
    fn generate_slot_id_dedups_with_numeric_suffix() {
        let taken = vec!["claude-sonnet-glm".to_string()];
        let id = generate_slot_id(AggregateTier::Sonnet, "GLM", "pid", &taken);
        assert_eq!(id, "claude-sonnet-glm-2");
        assert!(is_claude_safe_model_id(&id));
    }
}
```

在 `src-tauri/src/lib.rs` 的模块声明区加入 `mod aggregate;`（与既有 `mod` 声明同处，按字母序）。

- [ ] **Step 2: 运行测试确认失败**

```bash
export PATH="/c/Users/Jason/.cargo/bin:$PATH"
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
cargo test aggregate:: 2>&1 | tail -20
```
Expected: 编译错误 —— `is_claude_safe_model_id` 若未 pub 会报私有；否则测试应全部通过（本任务的实现已随测试一并给出，故此步的真实失败点通常只有可见性问题）。

> 若 `is_claude_safe_model_id` 不是 `pub`，Task 1 需先把它改成 `pub fn`（它在 `claude_desktop_config.rs`，已标 `pub`）。确认方式：`grep -n "pub fn is_claude_safe_model_id" src-tauri/src/claude_desktop_config.rs`。

- [ ] **Step 3: 运行测试确认通过**

```bash
cargo test aggregate:: 2>&1 | tail -12
```
Expected: `test result: ok. 6 passed`

- [ ] **Step 4: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git add src-tauri/src/aggregate.rs src-tauri/src/lib.rs
git commit -m "feat(aggregate): 槽位数据模型与槽位 ID 生成

纯逻辑：档位、槽位、默认目标三个类型，以及 claude-{tier}-{slug} 的
ID 生成（含中文名回落与编号去重），生成结果必须通过 is_claude_safe_model_id。"
```

---

### Task 2: 挂到 ProviderMeta（序列化契约）

**Files:**
- Modify: `src-tauri/src/provider.rs`（`ProviderMeta` 结构加字段）

**Interfaces:**
- Consumes: Task 1 的 `AggregateRoutes`
- Produces: `ProviderMeta.aggregate_routes: Option<AggregateRoutes>`（serde 名 `aggregateRoutes`，None 时不序列化）

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/provider.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn provider_meta_round_trips_aggregate_routes() {
        let meta: ProviderMeta = serde_json::from_value(serde_json::json!({
            "aggregateRoutes": {
                "slots": [
                    {
                        "routeId": "claude-sonnet-glm",
                        "tier": "sonnet",
                        "providerId": "p-glm",
                        "upstreamModel": "glm-5.3",
                        "label": "智谱 GLM-5.3",
                        "supports1m": true
                    }
                ],
                "defaultTarget": { "kind": "providerId", "value": "p-glm" }
            }
        }))
        .expect("deserialize");
        let routes = meta.aggregate_routes.expect("aggregate routes present");
        assert_eq!(routes.slots.len(), 1);
        // routeId 是必填的持久化路由键（前端生成、后端校验），往返测试需覆盖它
        assert_eq!(routes.slots[0].route_id, "claude-sonnet-glm");
        assert_eq!(routes.slots[0].upstream_model, "glm-5.3");
        assert_eq!(routes.slots[0].tier, crate::aggregate::AggregateTier::Sonnet);
        assert!(routes.slots[0].supports_1m);
        assert_eq!(
            routes.default_target,
            crate::aggregate::DefaultTarget::ProviderId("p-glm".to_string())
        );

        // 未设置时不得出现在序列化结果里
        let empty = ProviderMeta::default();
        let value = serde_json::to_value(&empty).expect("serialize");
        assert!(value.get("aggregateRoutes").is_none());
    }
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test provider_meta_round_trips_aggregate_routes 2>&1 | tail -12
```
Expected: FAIL —— `ProviderMeta` 无 `aggregate_routes` 字段（编译错误）

- [ ] **Step 3: 加字段**

在 `ProviderMeta` 内、`claude_desktop_model_routes` 字段之后插入：

```rust
    /// 聚合供应商的路由表：自身无端点无凭据，按模型把请求分流到其他供应商。
    /// None = 普通供应商。
    #[serde(
        default,
        rename = "aggregateRoutes",
        skip_serializing_if = "Option::is_none"
    )]
    pub aggregate_routes: Option<crate::aggregate::AggregateRoutes>,
```

若 `ProviderMeta` 实现了 `Default`（`derive` 或手写），确保新字段默认为 `None`——用 `derive(Default)` 时 `Option` 自动为 `None`；手写 `Default` 需补 `aggregate_routes: None,`。

- [ ] **Step 4: 运行确认通过**

```bash
cargo test provider_meta_round_trips_aggregate_routes 2>&1 | tail -8
cargo test provider:: 2>&1 | tail -5
```
Expected: 新测试 PASS，既有 provider 测试无回归

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/provider.rs
git commit -m "feat(aggregate): ProviderMeta 增加 aggregateRoutes 字段

None 时不序列化，普通供应商的 JSON 形状不变。"
```

---

### Task 3: 模型列表派生（profile 写入 + /models 端点）

**Files:**
- Modify: `src-tauri/src/claude_desktop_config.rs`（`proxy_model_routes` 与 `model_list_response` 两个出口）

**Interfaces:**
- Consumes: Task 2 的 `ProviderMeta.aggregate_routes`
- Produces: `crate::aggregate::is_aggregate_provider(&Provider) -> bool`；`crate::aggregate::aggregate_model_routes(&Provider) -> Result<Vec<ResolvedModelRoute>, AppError>`
- 影响：`proxy_model_routes(provider)` 与 `model_list_response(provider)` 对聚合供应商返回槽位派生的列表；对普通供应商**行为完全不变**

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/claude_desktop_config.rs` 的 `mod tests` 内追加（构造一个聚合供应商，断言派生出槽位对应的模型规格）：

```rust
    #[test]
    fn aggregate_provider_derives_model_routes_from_slots() {
        let mut provider = direct_provider("agg");
        let meta = provider.meta.get_or_insert_with(Default::default);
        meta.aggregate_routes = Some(crate::aggregate::AggregateRoutes {
            slots: vec![
                crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-sonnet-glm".into(),
                    tier: crate::aggregate::AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3".into(),
                    label: Some("智谱 GLM-5.3".into()),
                    supports_1m: true,
                },
                crate::aggregate::AggregateRouteSlot {
                    route_id: "claude-haiku-ds".into(),
                    tier: crate::aggregate::AggregateTier::Haiku,
                    provider_id: "p-ds".into(),
                    upstream_model: "deepseek-flash".into(),
                    label: None,
                    supports_1m: false,
                },
            ],
            default_target: crate::aggregate::DefaultTarget::ProviderId("p-glm".into()),
        });

        let routes = proxy_model_routes(&provider).expect("routes");
        // 按 route_id 排序（与既有实现一致）
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].route_id, "claude-haiku-ds");
        assert_eq!(routes[0].upstream_model, "deepseek-flash");
        assert_eq!(routes[0].label_override, None);
        assert!(!routes[0].supports_1m);
        assert_eq!(routes[1].route_id, "claude-sonnet-glm");
        assert_eq!(routes[1].upstream_model, "glm-5.3");
        assert_eq!(routes[1].label_override.as_deref(), Some("智谱 GLM-5.3"));
        assert!(routes[1].supports_1m);
    }
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test aggregate_provider_derives_model_routes_from_slots 2>&1 | tail -12
```
Expected: FAIL —— 聚合供应商目前会走 `claude_desktop_model_routes` 分支并报「缺少模型路由映射」

- [ ] **Step 3: 实现**

在 `src-tauri/src/aggregate.rs` 末尾追加：

```rust
use crate::claude_desktop_config::ResolvedModelRoute;
use crate::error::AppError;
use crate::provider::Provider;

/// 该供应商是否为聚合供应商。
pub fn is_aggregate_provider(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .is_some()
}

/// 由槽位派生模型规格（供 profile 的 inferenceModels 与 /models 端点共用）。
/// 槽位 ID 是保存时生成并持久化的，这里直接取用；按 route_id 排序与既有实现保持一致。
pub fn aggregate_model_routes(provider: &Provider) -> Result<Vec<ResolvedModelRoute>, AppError> {
    let routes = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.routes_missing",
                "聚合供应商缺少路由表",
                "Aggregate provider is missing its route table",
            )
        })?;

    let mut out = Vec::with_capacity(routes.slots.len());
    for slot in &routes.slots {
        let upstream = slot.upstream_model.trim();
        let route_id = slot.route_id.trim();
        if upstream.is_empty() || route_id.is_empty() {
            continue;
        }
        out.push(ResolvedModelRoute {
            route_id: route_id.to_string(),
            upstream_model: upstream.to_string(),
            label_override: slot
                .label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
            supports_1m: slot.supports_1m,
        });
    }
    out.sort_by(|a, b| a.route_id.cmp(&b.route_id));
    Ok(out)
}
```

在 `src-tauri/src/claude_desktop_config.rs` 的 `proxy_model_routes` **函数体最前面**插入分流：

```rust
    // 聚合供应商：模型规格由路由表的槽位派生，而不是自身的 claudeDesktopModelRoutes
    if crate::aggregate::is_aggregate_provider(provider) {
        return crate::aggregate::aggregate_model_routes(provider);
    }
```

在 `model_list_response` 内同样位置插入相同的分流（该函数同样以 `proxy_model_routes` 或等价逻辑取模型列表——按其实际实现插入到取列表之前）。

- [ ] **Step 4: 运行确认通过**

```bash
cargo test aggregate 2>&1 | tail -10
cargo test claude_desktop 2>&1 | tail -6
```
Expected: 新测试 PASS；既有 claude_desktop 测试无回归

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/aggregate.rs src-tauri/src/claude_desktop_config.rs
git commit -m "feat(aggregate): 模型列表由槽位派生

proxy_model_routes 与 model_list_response 对聚合供应商走新分支；
普通供应商行为不变。"
```

---

### Task 4: 请求路由（按模型切换目标供应商）

**Files:**
- Modify: `src-tauri/src/aggregate.rs`（加解析函数）
- Modify: `src-tauri/src/proxy/handler_context.rs`（选中供应商后分流）

**Interfaces:**
- Consumes: Task 2/3
- Produces: `aggregate::resolve_target(db, app_type, aggregate, request_model) -> Result<(Provider, Option<String>), AppError>`（`Option<String>` = 需改写的上游模型名）
- Produces: `HandlerContext.aggregate_override: Option<String>`（命中时记录上游模型名，供转发前改写 `body.model`）

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/aggregate.rs` 的 tests 内追加（用内存库构造目标供应商）：

```rust
    #[tokio::test]
    async fn resolve_target_hits_slot_by_generated_id() {
        let db = crate::database::Database::memory().expect("db");
        // 目标供应商（用仓库既有惯例 Provider::with_id 构造，Provider 未 derive Default）
        let target = crate::provider::Provider::with_id(
            "p-glm".to_string(),
            "GLM".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save target");

        let mut aggregate = crate::provider::Provider::with_id(
            "agg".to_string(),
            "Aggregate".to_string(),
            serde_json::json!({}),
            None,
        );
        aggregate.meta = Some(crate::provider::ProviderMeta {
            aggregate_routes: Some(AggregateRoutes {
                slots: vec![AggregateRouteSlot {
                    route_id: "claude-sonnet-glm".into(),
                    tier: AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3".into(),
                    label: None,
                    supports_1m: false,
                }],
                default_target: DefaultTarget::ProviderId("p-glm".into()),
            }),
            ..Default::default()
        });

        let hit = resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-glm")
            .expect("hit");
        assert_eq!(hit.0.id, "p-glm");
        assert_eq!(hit.1.as_deref(), Some("glm-5.3"));

        // 未命中 → 默认目标，且不改写模型名
        let miss = resolve_target(&db, "claude-desktop", &aggregate, "claude-haiku-4-5")
            .expect("miss falls back to default");
        assert_eq!(miss.0.id, "p-glm");
        assert_eq!(miss.1, None);
    }
```

> `db.save_provider` / `Database::memory()` 的签名以仓库既有测试为准（`provider_router.rs` 与 `claude_desktop_config.rs` 的 tests 里都有用法可参照）。

- [ ] **Step 2: 运行确认失败**

```bash
cargo test resolve_target_hits_slot_by_generated_id 2>&1 | tail -12
```
Expected: FAIL —— `resolve_target` 未定义

- [ ] **Step 3: 实现**

在 `src-tauri/src/aggregate.rs` 追加：

```rust
/// 解析聚合路由：给定聚合供应商与请求模型，返回 (目标供应商, 需改写的上游模型名)。
/// - 命中槽位 → 返回该槽位的目标与上游模型
/// - 未命中 → 返回默认目标，模型名不改写（None）
/// - 默认目标也不可用 → 返回明确错误（不静默降级）
pub fn resolve_target(
    db: &crate::database::Database,
    app_type: &str,
    aggregate: &Provider,
    request_model: &str,
) -> Result<(Provider, Option<String>), AppError> {
    let routes = aggregate
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.routes_missing",
                "聚合供应商缺少路由表",
                "Aggregate provider is missing its route table",
            )
        })?;

    // 直接按持久化的槽位 ID 查表（ID 在保存时生成，运行时不重新生成）
    for slot in &routes.slots {
        if slot.route_id == request_model {
            let target = load_provider(db, app_type, &slot.provider_id)?;
            return Ok((target, Some(slot.upstream_model.clone())));
        }
    }

    // 未命中 → 默认目标
    let fallback_id = match &routes.default_target {
        DefaultTarget::ProviderId(id) => id.clone(),
        DefaultTarget::SlotId(slot_id) => routes
            .slots
            .iter()
            .find(|slot| &slot.route_id == slot_id)
            .map(|slot| slot.provider_id.clone())
            .ok_or_else(|| {
                AppError::localized(
                    "aggregate.default_target_slot_missing",
                    "聚合供应商的默认目标指向了不存在的槽位",
                    "Aggregate default target points to a missing slot",
                )
            })?,
    };
    let target = load_provider(db, app_type, &fallback_id)?;
    Ok((target, None))
}

fn load_provider(
    db: &crate::database::Database,
    app_type: &str,
    provider_id: &str,
) -> Result<Provider, AppError> {
    db.get_provider_by_id(provider_id, app_type)?
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.target_provider_missing",
                "聚合供应商的目标供应商不存在",
                "Aggregate target provider does not exist",
            )
        })
}
```

在 `src-tauri/src/proxy/handler_context.rs` 的 `let provider = providers.first().cloned()...` 之后（第 146–149 行之后）插入分流，并把结果存入结构体：

```rust
        // 聚合供应商：按请求模型把「本次使用的供应商」换成路由表里的目标供应商
        let (provider, aggregate_override) = if crate::aggregate::is_aggregate_provider(&provider) {
            let (target, upstream) =
                crate::aggregate::resolve_target(&state.db, app_type_str, &provider, &request_model)
                    .map_err(|e| ProxyError::ConfigError(e.to_string()))?;
            log::debug!(
                "[{}] Aggregate route: {} -> provider {} (upstream model: {:?})",
                tag,
                request_model,
                target.name,
                upstream
            );
            (target, upstream)
        } else {
            (provider, None)
        };
```

在 `HandlerContext` 结构体定义中（`outbound_model` 附近）加字段：

```rust
    /// 聚合路由命中时需改写的上游模型名（None = 不改写，走既有映射）
    pub aggregate_override: Option<String>,
```

并在构造 `Ok(Self { ... })` 时加入 `aggregate_override,`。

- [ ] **Step 4: 在转发前应用改写**

在 `src-tauri/src/proxy/forwarder.rs` 中，`map_proxy_request_model` 被调用的位置（约 1249 行）之前插入分流：

```rust
        // 聚合路由命中：模型映射由路由表给出，不再走目标供应商自己的路由表
        if let Some(upstream) = ctx.aggregate_override.as_deref() {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("model".to_string(), serde_json::Value::String(upstream.to_string()));
            }
        }
```

> `body` 在此处已是可变局部副本（该函数开头会 clone/取得 owned body）；若实际为借用，改为在其 clone 之后插入同样的语句。

- [ ] **Step 5: 运行确认通过**

```bash
cargo test aggregate 2>&1 | tail -10
cargo test proxy:: 2>&1 | tail -8
```
Expected: 新测试 PASS；proxy 既有测试无回归

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/aggregate.rs src-tauri/src/proxy/handler_context.rs src-tauri/src/proxy/forwarder.rs
git commit -m "feat(aggregate): 按请求模型路由到目标供应商

handler_context 选供应商后分流：命中槽位则换成目标供应商并把目标写入
aggregate_override，转发前据此改写 body.model；未命中走默认目标。"
```

---

### Task 5: 保存校验与删除保护

**Files:**
- Modify: `src-tauri/src/services/provider/mod.rs`（`validate_provider_settings` 与删除路径）

**Interfaces:**
- Consumes: Task 1/2
- Produces: 保存聚合供应商时的校验（至少一个槽位、默认目标有效、禁嵌套、生成 ID 合法）；删除被引用供应商时被拒绝

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/services/provider/mod.rs` 的 tests 内追加：

```rust
    /// 构造一个带聚合路由表的供应商（Provider 未 derive Default，用 with_id）
    fn aggregate_provider(id: &str, routes: crate::aggregate::AggregateRoutes) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            "Aggregate".to_string(),
            serde_json::json!({}),
            None,
        );
        provider.meta = Some(ProviderMeta {
            aggregate_routes: Some(routes),
            ..Default::default()
        });
        provider
    }

    fn slot(route_id: &str, provider_id: &str) -> crate::aggregate::AggregateRouteSlot {
        crate::aggregate::AggregateRouteSlot {
            route_id: route_id.into(),
            tier: crate::aggregate::AggregateTier::Sonnet,
            provider_id: provider_id.into(),
            upstream_model: "m".into(),
            label: None,
            supports_1m: false,
        }
    }

    #[test]
    fn validate_aggregate_rejects_empty_slots() {
        let provider = aggregate_provider(
            "agg",
            crate::aggregate::AggregateRoutes {
                slots: vec![],
                default_target: crate::aggregate::DefaultTarget::ProviderId("x".into()),
            },
        );
        let err = validate_provider_settings(&AppType::ClaudeDesktop, &provider)
            .expect_err("empty slots must be rejected");
        assert!(err.to_string().contains("槽位"), "unexpected error: {err}");
    }

    #[test]
    fn validate_aggregate_rejects_self_reference() {
        let provider = aggregate_provider(
            "agg",
            crate::aggregate::AggregateRoutes {
                slots: vec![slot("claude-sonnet-agg", "agg")], // 指向自己
                default_target: crate::aggregate::DefaultTarget::ProviderId("agg".into()),
            },
        );
        let err = validate_provider_settings(&AppType::ClaudeDesktop, &provider)
            .expect_err("self reference must be rejected");
        assert!(err.to_string().contains("自身"), "unexpected error: {err}");
    }

    #[test]
    fn validate_aggregate_rejects_duplicate_route_ids() {
        let provider = aggregate_provider(
            "agg",
            crate::aggregate::AggregateRoutes {
                slots: vec![slot("claude-sonnet-a", "p1"), slot("claude-sonnet-a", "p1")],
                default_target: crate::aggregate::DefaultTarget::ProviderId("p1".into()),
            },
        );
        assert!(validate_provider_settings(&AppType::ClaudeDesktop, &provider).is_err());
    }

    #[test]
    fn validate_aggregate_rejects_unsafe_route_id() {
        // 缺少角色前缀的 ID 会被 Claude Desktop 整组拒收
        let provider = aggregate_provider(
            "agg",
            crate::aggregate::AggregateRoutes {
                slots: vec![slot("glm-5.3", "p1")],
                default_target: crate::aggregate::DefaultTarget::ProviderId("p1".into()),
            },
        );
        let err = validate_provider_settings(&AppType::ClaudeDesktop, &provider)
            .expect_err("unsafe route id must be rejected");
        assert!(err.to_string().contains("槽位 ID"), "unexpected error: {err}");
    }
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test validate_aggregate 2>&1 | tail -12
```
Expected: FAIL —— 目前没有任何聚合相关校验

- [ ] **Step 3: 实现**

在 `validate_provider_settings` 内、既有校验之后追加：

```rust
    // 聚合供应商校验
    if let Some(routes) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
    {
        if routes.slots.is_empty() {
            return Err(AppError::localized(
                "aggregate.slots_empty",
                "聚合供应商至少需要一个槽位",
                "Aggregate provider requires at least one slot",
            ));
        }
        let mut seen: Vec<String> = Vec::new();
        for slot in &routes.slots {
            if slot.provider_id.trim().is_empty() || slot.upstream_model.trim().is_empty() {
                return Err(AppError::localized(
                    "aggregate.slot_incomplete",
                    "槽位必须同时指定目标供应商与上游模型",
                    "Each slot must specify both a target provider and an upstream model",
                ));
            }
            if slot.provider_id == provider.id {
                return Err(AppError::localized(
                    "aggregate.self_reference",
                    "聚合供应商不能把自身作为目标",
                    "An aggregate provider cannot target itself",
                ));
            }
            // 槽位 ID 由前端生成（预览即实际值），这里只做校验
            let route_id = slot.route_id.trim();
            if route_id.is_empty()
                || !crate::claude_desktop_config::is_claude_safe_model_id(route_id)
            {
                return Err(AppError::localized(
                    "aggregate.invalid_route_id",
                    "槽位 ID 不合法（须形如 claude-sonnet-glm）；Claude Desktop 会整组拒收",
                    "Invalid slot id (expected e.g. claude-sonnet-glm); Claude Desktop would reject the whole group",
                ));
            }
            if seen.iter().any(|s| s == route_id) {
                return Err(AppError::localized(
                    "aggregate.duplicate_route_id",
                    "槽位 ID 重复",
                    "Duplicate slot id",
                ));
            }
            seen.push(route_id.to_string());
        }
    }
```

> 校验信息须含「槽位」（空槽位用例）与「自身」（自引用用例）与「槽位 ID」（非法 ID 用例），以匹配 Task 5 Step 1 的断言。

**注**：槽位 ID 由**前端**在编辑时生成并随表单提交（Task 6 的 `assignSlotIds` + Task 7 的预览），后端仅校验——这样界面上的预览就是实际生效的值，且校验层无需访问数据库。**「禁嵌套」**（目标供应商自身是聚合供应商）需要跨供应商信息，放在**保存服务层**（能访问 DB 的那一层）：在保存前扫描该 app 下所有供应商，若某槽位的 `provider_id` 指向的供应商带 `aggregate_routes`，则拒绝保存；该检查的测试同样写在服务层测试中。
```

**删除保护**：在删除供应商的服务函数内、实际删除之前，扫描同 app 下所有供应商的 `aggregate_routes.slots`，若有引用则拒绝：

```rust
    // 删除保护：被聚合路由表引用的供应商不可删
    let all = db.get_providers(app_type)?;
    for other in &all {
        let Some(routes) = other
            .meta
            .as_ref()
            .and_then(|meta| meta.aggregate_routes.as_ref())
        else {
            continue;
        };
        if routes
            .slots
            .iter()
            .any(|slot| slot.provider_id == provider_id)
        {
            return Err(AppError::localized(
                "aggregate.provider_in_use",
                "该供应商被聚合供应商引用，需先移除对应槽位",
                "This provider is referenced by an aggregate provider; remove the slot first",
            ));
        }
    }
```

（`db.get_providers(app_type)` 的具体签名以仓库既有 API 为准。）

- [ ] **Step 4: 运行确认通过**

```bash
cargo test validate_aggregate 2>&1 | tail -10
cargo test provider_service 2>&1 | tail -6
```
Expected: 新测试 PASS；既有测试无回归

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/provider/mod.rs
git commit -m "feat(aggregate): 保存校验与删除保护

校验至少一个槽位、目标与模型必填、禁自引用、生成的槽位 ID 合法；
被聚合路由引用的供应商不可删除。"
```

---

### Task 6: 前端类型与槽位 ID 生成（与 Rust 保持一致）

**Files:**
- Modify: `src/types.ts`
- Create: `src/utils/aggregateRoutes.ts`
- Create: `src/utils/aggregateRoutes.test.ts`

**Interfaces:**
- Consumes: 无（纯前端）
- Produces: `AggregateTier`、`AggregateRouteSlot`、`AggregateRoutes`、`DefaultTarget` 类型（挂到 `ProviderMeta.aggregateRoutes`）；`slugify(name, id)`、`generateSlotId(tier, name, id, taken)`（**必须与 Task 1 的 Rust 实现逐字等价**）

- [ ] **Step 1: 写失败测试**

创建 `src/utils/aggregateRoutes.test.ts`：

```ts
import { describe, expect, it } from "vitest";
import { generateSlotId, slugify } from "./aggregateRoutes";

describe("slugify", () => {
  it("规范 ASCII 名称", () => {
    expect(slugify("DeepSeek-OTN", "abc12345")).toBe("deepseek-otn");
    expect(slugify("OpenCode Go", "abc12345")).toBe("opencode-go");
  });

  it("混合名取 ASCII 部分", () => {
    expect(slugify("智谱 GLM", "97a1d0df-b9a9")).toBe("glm");
  });

  it("纯非 ASCII 名回落为 provider id 前缀", () => {
    expect(slugify("月之暗面", "97a1d0df-b9a9")).toBe("97a1d0df");
  });

  it("截断并清理首尾分隔符", () => {
    expect(slugify("  ---A--B---  ", "x")).toBe("a-b");
    expect(slugify("abcdefghijklmnopqrstuvwxyz", "x")).toHaveLength(20);
  });
});

describe("generateSlotId", () => {
  it("生成 claude-{tier}-{slug}", () => {
    expect(generateSlotId("sonnet", "DeepSeek-OTN", "pid", [])).toBe(
      "claude-sonnet-deepseek-otn",
    );
  });

  it("冲突时追加编号", () => {
    expect(
      generateSlotId("sonnet", "GLM", "pid", ["claude-sonnet-glm"]),
    ).toBe("claude-sonnet-glm-2");
  });
});

describe("assignSlotIds", () => {
  const providers = [
    { id: "p-glm", name: "智谱 GLM" },
    { id: "p-ds", name: "DeepSeek-OTN" },
  ];
  const base = {
    defaultTarget: { kind: "providerId" as const, value: "p-glm" },
  };

  it("按目标供应商名称生成 ID，并处理同名冲突", () => {
    const next = assignSlotIds(
      {
        ...base,
        slots: [
          { routeId: "", tier: "sonnet", providerId: "p-ds", upstreamModel: "flash" },
          { routeId: "", tier: "sonnet", providerId: "p-ds", upstreamModel: "pro" },
        ],
      },
      providers,
    );
    // "DeepSeek-OTN" → slug "deepseek-otn"；第二条同供应商 → 追加编号
    expect(next.slots[0].routeId).toBe("claude-sonnet-deepseek-otn");
    expect(next.slots[1].routeId).toBe("claude-sonnet-deepseek-otn-2");
  });

  it("档位变化会改变 ID", () => {
    const next = assignSlotIds(
      {
        ...base,
        slots: [
          { routeId: "", tier: "opus", providerId: "p-ds", upstreamModel: "flash" },
        ],
      },
      providers,
    );
    expect(next.slots[0].routeId).toBe("claude-opus-deepseek-otn");
  });
});
```

- [ ] **Step 2: 运行确认失败**

```bash
cd "D:/Workspace/Project/cc-switch/src"
pnpm test:unit -- aggregateRoutes 2>&1 | tail -15
```
Expected: FAIL —— 模块不存在

- [ ] **Step 3: 实现**

创建 `src/utils/aggregateRoutes.ts`：

```ts
import type { AggregateRoutes, AggregateTier, Provider } from "@/types";

/** 与后端 aggregate.rs 的 slugify 逐字等价：转小写 → 非字母数字替换为 '-' →
 *  合并连续 '-' → 去首尾 '-' → 截断 20 字符；结果为空则回落为 provider id 前 8 位。 */
export function slugify(providerName: string, providerId: string): string {
  let out = "";
  let lastDash = false;
  for (const ch of providerName) {
    if (/[A-Za-z0-9]/.test(ch)) {
      out += ch.toLowerCase();
      lastDash = false;
    } else if (!lastDash && out.length > 0) {
      out += "-";
      lastDash = true;
    }
  }
  const trimmed = out.replace(/^-+|-+$/g, "");
  const truncated = trimmed.slice(0, 20).replace(/-+$/g, "");
  return truncated.length > 0 ? truncated : providerId.slice(0, 8);
}

/** 与后端 generate_slot_id 等价。 */
export function generateSlotId(
  tier: AggregateTier,
  providerName: string,
  providerId: string,
  taken: string[],
): string {
  const base = `claude-${tier}-${slugify(providerName, providerId)}`;
  if (!taken.includes(base)) return base;
  let n = 2;
  for (;;) {
    const candidate = `${base}-${n}`;
    if (!taken.includes(candidate)) return candidate;
    n += 1;
  }
}

/** 供应商是否为聚合供应商。 */
export function isAggregateProvider(provider: Pick<Provider, "meta">): boolean {
  return Boolean(provider.meta?.aggregateRoutes);
}

/** 由槽位派生标签（选择器显示名），缺省用上游模型名。 */
export function slotLabel(slot: {
  upstreamModel: string;
  label?: string;
}): string {
  const l = slot.label?.trim();
  return l && l.length > 0 ? l : slot.upstreamModel;
}

/** 路由表是否可保存（至少一个槽位）。 */
export function canSaveAggregateRoutes(
  routes: AggregateRoutes | undefined | null,
): boolean {
  return Boolean(routes && routes.slots.length > 0 && routes.defaultTarget);
}

/** 为所有槽位重新生成 routeId（按当前顺序去重），返回新的路由表。
 *  在槽位增删、目标供应商变更、档位变更后调用——保证预览与实际提交值一致。 */
export function assignSlotIds(
  routes: AggregateRoutes,
  providers: Pick<Provider, "id" | "name">[],
): AggregateRoutes {
  const taken: string[] = [];
  const slots = routes.slots.map((slot) => {
    const provider = providers.find((p) => p.id === slot.providerId);
    const routeId = generateSlotId(
      slot.tier,
      provider?.name ?? slot.providerId,
      slot.providerId,
      taken,
    );
    taken.push(routeId);
    return { ...slot, routeId };
  });
  return { ...routes, slots };
}
```

在 `src/types.ts` 的 `ProviderMeta` 接口内（`claudeDesktopModelRoutes` 之后）追加：

```ts
  // 聚合供应商：无端点无凭据，按模型把请求分流到其他供应商
  aggregateRoutes?: AggregateRoutes;
```

并在 `src/types.ts` 内新增类型：

```ts
export type AggregateTier = "sonnet" | "opus" | "haiku" | "fable";

export interface AggregateRouteSlot {
  /** 生成的槽位 ID（持久化，稳定的路由键）。由 assignSlotIds 写入，后端只校验。 */
  routeId: string;
  tier: AggregateTier;
  providerId: string;
  upstreamModel: string;
  label?: string;
  supports1m?: boolean;
}

export type DefaultTarget =
  | { kind: "slotId"; value: string }
  | { kind: "providerId"; value: string };

export interface AggregateRoutes {
  slots: AggregateRouteSlot[];
  defaultTarget: DefaultTarget;
}
```

- [ ] **Step 4: 运行确认通过**

```bash
pnpm test:unit -- aggregateRoutes 2>&1 | tail -8
pnpm typecheck 2>&1 | tail -5
```
Expected: vitest 全过；typecheck 无错误

- [ ] **Step 5: 提交**

```bash
git add src/types.ts src/utils/aggregateRoutes.ts src/utils/aggregateRoutes.test.ts
git commit -m "feat(aggregate): 前端类型与槽位 ID 生成

slugify / generateSlotId 与后端 aggregate.rs 逐字等价（含中文名回落与编号去重），
供 UI 实时预览槽位 ID。"
```

---

### Task 7: 配置界面

**Files:**
- Create: `src/components/providers/forms/AggregateProviderFields.tsx`
- Modify: `src/components/providers/forms/ClaudeDesktopProviderForm.tsx`（把聚合槽位编辑器接入）
- Modify: `src/i18n/locales/zh.json`、`src/i18n/locales/en.json`

**Interfaces:**
- Consumes: Task 6 的类型与工具函数
- Produces: `<AggregateProviderFields value={routes} onChange={...} providers={...} />`

- [ ] **Step 1: 实现组件**

创建 `src/components/providers/forms/AggregateProviderFields.tsx`：

```tsx
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useTranslation } from "react-i18next";
import type {
  AggregateRouteSlot,
  AggregateRoutes,
  AggregateTier,
  DefaultTarget,
  Provider,
} from "@/types";
import { assignSlotIds, slotLabel } from "@/utils/aggregateRoutes";

const TIERS: AggregateTier[] = ["sonnet", "opus", "haiku", "fable"];

interface Props {
  value: AggregateRoutes;
  onChange: (next: AggregateRoutes) => void;
  /** 可作目标的常规供应商（不含聚合供应商自身，防止嵌套） */
  candidates: Provider[];
}

export function AggregateProviderFields({ value, onChange, candidates }: Props) {
  const { t } = useTranslation();

  // 每次变更都重算所有槽位 ID：保证「预览 = 实际提交值」
  const commit = (next: AggregateRoutes) => onChange(assignSlotIds(next, candidates));

  const setSlot = (index: number, patch: Partial<AggregateRouteSlot>) => {
    const slots = value.slots.map((s, i) => (i === index ? { ...s, ...patch } : s));
    commit({ ...value, slots });
  };
  const addSlot = () =>
    commit({
      ...value,
      slots: [
        ...value.slots,
        {
          routeId: "",
          tier: "sonnet",
          providerId: candidates[0]?.id ?? "",
          upstreamModel: "",
          supports1m: false,
        },
      ],
    });
  const removeSlot = (index: number) =>
    commit({ ...value, slots: value.slots.filter((_, i) => i !== index) });

  return (
    <section className="space-y-4">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">{t("aggregate.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("aggregate.hint")}</p>
      </header>

      <div className="space-y-3">
        {value.slots.map((slot, index) => (
          <div key={index} className="space-y-2 rounded-md border border-border-default p-3">
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <Label className="text-xs">{t("aggregate.tier")}</Label>
                <Select
                  value={slot.tier}
                  onValueChange={(v) => setSlot(index, { tier: v as AggregateTier })}
                >
                  <SelectTrigger className="h-8"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {TIERS.map((tier) => (
                      <SelectItem key={tier} value={tier}>{tier}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1">
                <Label className="text-xs">{t("aggregate.targetProvider")}</Label>
                <Select
                  value={slot.providerId}
                  onValueChange={(v) => setSlot(index, { providerId: v })}
                >
                  <SelectTrigger className="h-8"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {candidates.map((p) => (
                      <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <Label className="text-xs">{t("aggregate.upstreamModel")}</Label>
                <Input
                  className="h-8"
                  value={slot.upstreamModel}
                  onChange={(e) => setSlot(index, { upstreamModel: e.target.value })}
                />
              </div>
              <div className="space-y-1">
                <Label className="text-xs">{t("aggregate.displayName")}</Label>
                <Input
                  className="h-8"
                  placeholder={slotLabel(slot)}
                  value={slot.label ?? ""}
                  onChange={(e) => setSlot(index, { label: e.target.value })}
                />
              </div>
            </div>

            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Switch
                  id={`agg-1m-${index}`}
                  checked={Boolean(slot.supports1m)}
                  onCheckedChange={(v) => setSlot(index, { supports1m: v })}
                />
                <Label htmlFor={`agg-1m-${index}`} className="text-xs">
                  {t("aggregate.supports1m")}
                </Label>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => removeSlot(index)}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>

            <p className="text-xs text-muted-foreground">
              {t("aggregate.slotIdPreview")}: <code>{slot.routeId}</code>
            </p>
          </div>
        ))}
      </div>

      <Button type="button" variant="outline" size="sm" className="h-8 gap-1.5" onClick={addSlot}>
        <Plus className="h-3.5 w-3.5" />
        {t("aggregate.addSlot")}
      </Button>

      <div className="space-y-1">
        <Label className="text-xs">{t("aggregate.defaultTarget")}</Label>
        <Select
          value={
            value.defaultTarget.kind === "providerId"
              ? `provider:${value.defaultTarget.value}`
              : `slot:${value.defaultTarget.value}`
          }
          onValueChange={(v) => {
            const [kind, ...rest] = v.split(":");
            const target: DefaultTarget =
              kind === "provider"
                ? { kind: "providerId", value: rest.join(":") }
                : { kind: "slotId", value: rest.join(":") };
            onChange({ ...value, defaultTarget: target });
          }}
        >
          <SelectTrigger className="h-8"><SelectValue /></SelectTrigger>
          <SelectContent>
            {candidates.map((p) => (
              <SelectItem key={p.id} value={`provider:${p.id}`}>{p.name}</SelectItem>
            ))}
            {value.slots.map((s, index) => (
              <SelectItem key={s.routeId || `slot-${index}`} value={`slot:${s.routeId}`}>
                {slotLabel(s)} ({s.routeId})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">{t("aggregate.defaultTargetHint")}</p>
      </div>
    </section>
  );
}
```

> 若 `@/components/ui/select` 不存在，改用仓库既有的下拉组件（先 `ls src/components/ui/` 确认）；不要新增依赖。

- [ ] **Step 2: 接入 ClaudeDesktopProviderForm**

在 `ClaudeDesktopProviderForm.tsx` 内：当该供应商的 `meta.aggregateRoutes` 启用时渲染 `<AggregateProviderFields>`。具体接入方式：新增一个「聚合供应商」开关（或复用该表单已有的模式选择），开启时初始化 `{ slots: [], defaultTarget: { kind: "providerId", value: "" } }`，并把 `candidates` 传为「同 app 下所有**非聚合**供应商」。

- [ ] **Step 3: 加 i18n**

`src/i18n/locales/zh.json` 与 `en.json` 的 `settings` 同级新增 `aggregate` 对象：

```jsonc
// zh.json
"aggregate": {
  "title": "聚合供应商",
  "hint": "自身不存端点与密钥，按模型把请求分流到其他供应商。启用后需重启 Claude Desktop 生效。",
  "tier": "档位",
  "targetProvider": "目标供应商",
  "upstreamModel": "上游模型",
  "displayName": "显示名",
  "supports1m": "1M 上下文",
  "slotIdPreview": "槽位 ID",
  "addSlot": "添加槽位",
  "defaultTarget": "默认目标",
  "defaultTargetHint": "未命中路由的请求发往这里（Claude 的内部调用会走它）"
}
// en.json
"aggregate": {
  "title": "Aggregate provider",
  "hint": "Holds no endpoint or key; routes requests to other providers by model. Restart Claude Desktop after enabling.",
  "tier": "Tier",
  "targetProvider": "Target provider",
  "upstreamModel": "Upstream model",
  "displayName": "Display name",
  "supports1m": "1M context",
  "slotIdPreview": "Slot ID",
  "addSlot": "Add slot",
  "defaultTarget": "Default target",
  "defaultTargetHint": "Requests that match no slot go here (Claude's internal calls use it)"
}
```

- [ ] **Step 4: 验证**

```bash
pnpm typecheck 2>&1 | tail -5
pnpm test:unit -- aggregateRoutes 2>&1 | tail -5
```
Expected: 均通过

- [ ] **Step 5: 提交**

```bash
git add src/components/providers/forms/AggregateProviderFields.tsx \
        src/components/providers/forms/ClaudeDesktopProviderForm.tsx \
        src/i18n/locales/zh.json src/i18n/locales/en.json
git commit -m "feat(aggregate): 槽位配置界面

槽位行编辑器（档位/目标供应商/上游模型/显示名/1M）+ 槽位 ID 实时预览 +
默认目标选择；接入 Claude Desktop 供应商表单，双语 i18n。"
```

---

### Task 8: 构建、部署与验收

**Files:** 无代码改动（构建与验证）

**Interfaces:**
- Consumes: Task 1–7 全部改动
- Produces: 本机可用的新构建

- [ ] **Step 1: 全量测试**

```bash
export PATH="/c/Users/Jason/.cargo/bin:$PATH"
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
cargo test 2>&1 | tail -20
```
Expected: 全绿。若有个别失败，须确认在改动前的 HEAD 上同样失败（预存失败）才可豁免。

- [ ] **Step 2: 前端检查**

```bash
cd "D:/Workspace/Project/cc-switch/src"
pnpm typecheck && pnpm test:unit 2>&1 | tail -8
```

- [ ] **Step 3: 构建**

```bash
cd "D:/Workspace/Project/cc-switch/src"
export PATH="/c/Users/Jason/.cargo/bin:$PATH"
export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR="https://gh-proxy.com/https://github.com/"
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy
pnpm tauri build --bundles nsis 2>&1 | tail -10
```

- [ ] **Step 4: 部署**

```bash
MSYS_NO_PATHCONV=1 taskkill /F /IM cc-switch.exe
```

```bash
MSYS_NO_PATHCONV=1 cmd /c "D:\Workspace\Project\cc-switch\tools\install-local.bat"
```

```bash
cmd //c start "" "C:\Users\Jason\AppData\Local\Programs\CC Switch\cc-switch.exe"
```

- [ ] **Step 5: 手工验收（需用户操作）**

1. 在 CC Switch 新建一个**聚合供应商**，添加 ≥2 个槽位，指向**不同**供应商（如 智谱 GLM-5.3、DeepSeek-V4）
2. 启用它 → 提示需重启 Claude Desktop → 重启
3. 打开模型选择器：应能看到配置的槽位（**注意 `supports1m` 会让每个槽位多出一行**）
4. 逐个选不同供应商的模型各发一条消息
5. 在 CC Switch 日志中确认请求分别打到了对应供应商的域名（`>>> 请求目标: https://...`）
6. 清理验证残留：把第 1 步创建的聚合供应商删除，或删掉探针槽位

**通过判据**：不同槽位的请求确实命中不同上游域名，且 CC Switch 的「当前供应商」始终显示为聚合供应商（不随请求跳动）。

- [ ] **Step 6: 提交（若有验收中的修正）**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git add -A && git commit -m "fix(aggregate): 验收中发现的问题修正"
```

---

## Self-Review 记录

- **Spec 覆盖**：需求与调研（§1–2，本计划为落地设计）· 平台约束（填入 Global Constraints）· 数据模型（Task 1–2）· 路由流程（Task 4）· Desktop 暴露（Task 3）· 配置界面（Task 6–7）· 边界与校验（Task 5）· 可行性验证（已在设计阶段完成，见 spec §10）· 测试策略（各任务内嵌）· 与既有补丁关系（Global Constraints 的分支约束 + Task 8 的部署）
- **占位符扫描**：无 TBD/TODO；两处「按仓库实际 API 调整」是对既有代码签名的**兼容性指示**（附有明确的确认命令或替换位置），不是待填内容。
- **类型一致性**：`AggregateTier` / `AggregateRouteSlot` / `AggregateRoutes` / `DefaultTarget` 在 Task 1（Rust）与 Task 6（TS）两处定义，字段名经 `camelCase` 对齐（`routeId` / `providerId` / `upstreamModel` / `supports1m` / `defaultTarget`）；`generate_slot_id` 与 `generateSlotId` 同名同语义；`resolve_target` 仅在 Task 4 定义与使用。

- **自检中修正的两处设计问题**（写计划时发现，已就地改掉）：
  1. **槽位 ID 改为「生成后持久化」**。原设计意图是"运行按需生成"，但运行时只有 `provider_id`、拿不到供应商**名称**，slug 会退化成 id，与已确认的「用名称短标识」不符。改为：**前端在编辑时生成并随表单提交**（`assignSlotIds`），后端只校验（Task 5）。附带收益——设计 §13 里「重命名供应商会导致槽位 ID 变化」这条风险**消失了**（ID 一旦生成即固定）；代价是重命名后 ID 不再跟随新名称，但这正是我们要的稳定性。
  2. **默认目标按槽位 ID 引用**（原为下标），避免槽位增删后错位。
  > 这两点需同步回设计文档 §5 与 §13。
