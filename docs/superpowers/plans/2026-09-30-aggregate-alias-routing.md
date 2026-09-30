# 聚合别名路由实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 spec `docs/superpowers/specs/2026-09-30-aggregate-alias-routing-design.md` 落地聚合路由的别名规则层：请求名未命中槽位时按前缀（大小写不敏感、按序先赢）转投指定槽位，UI 可配，根治平台别名撞贵槽/落兜底漂移。

**Architecture:** Rust `resolve_target` 在「精确槽位命中」与「兜底」之间插一层前缀规则匹配（命中即复用槽位的供应商+上游模型直给）；TS `AggregateRoutes` 增 `aliasRules`（serde default 零迁移）；`assignSlotIds` 让规则 slotId 跟随重编号；编辑器加「别名路由」区块。

**Tech Stack:** Rust + Tauri v2（cargo test）、React + TS（vitest + testing-library）、CC Switch DB（SQLite）、Claude Desktop 3P 代理。

## Global Constraints

- 分支 `fix/profile-merge`，只推 `origin`（Chris-Xie369/cc-switch），**绝不推 upstream**
- 提交信息**不含任何署名行**（无 Co-Authored-By 等——项目既有规则）
- 全程中文报告；DB/profile 含真实密钥，任何输出不复述密钥
- cargo 不在默认 PATH：`export PATH="/c/Users/Jason/.cargo/bin:$PATH"`
- 测试环境 i18n 是空资源：组件代码里**所有 `t()` 必须带 defaultValue**（诚实化计划踩过）
- Rust `#[serde(default)]` 零迁移；悬空别名静默跳过（不报错），兜底槽缺失仍显式报错（既有分级）
- 工作目录：`D:/Workspace/Project/cc-switch/src`（git 仓库根）
- **执行派发模型指导**：subagent 用平台别名 `sonnet`/`haiku`（现已落免费兜底 space-bunny）；**禁用 `fable`/`opus`**（撞真槽 k3/付费 DeepSeek 槽——fable→fable-4→k3，opus→opus-4-8→deepseek-flash）

---

### Task 1: Rust 数据结构 + resolve_target 别名层（TDD）

**Files:**
- Modify: `src-tauri/src/aggregate.rs`（新增结构体、`AggregateRoutes` 字段、`resolve_target` 插层、测试）
- Modify: 全库 `AggregateRoutes {` 字面量构造处补 `alias_rules: vec![]`（`grep -rn "AggregateRoutes {" src-tauri/src/` 定位，实测 22 处：proxy/server.rs、services/provider/mod.rs、services/stream_check.rs、aggregate.rs 测试、claude_desktop_config.rs 测试）

**Interfaces:**
- Consumes: 现有 `AggregateRoutes`（serde camelCase）、`resolve_target(db, app_type, aggregate, request_model) -> Result<(Provider, Option<String>)>`、测试辅助 `aggregate_with(slots, default_target)` / `slot_for(route_id, provider_id, tier, upstream_model)`、`Provider::with_id` + `db.save_provider("claude-desktop", …)` 既有模式
- Produces:
  - `pub struct AggregateAliasRule { pub prefix: String, pub slot_id: String }`（serde camelCase）
  - `AggregateRoutes.alias_rules: Vec<AggregateAliasRule>`（`#[serde(default)]`）
  - 路由序：剥 [1m] → 精确槽位 → **别名前缀（按序、小写化）** → 兜底

- [ ] **Step 1: 写失败测试（aggregate.rs tests 模块内，追加在既有 resolve_target 用例之后）**

```rust
    #[tokio::test]
    async fn alias_prefix_hits_route_to_slot_upstream() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-oc".to_string(),
            "OpenCode".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-oc",
                    AggregateTier::Sonnet,
                    "space-bunny-free",
                )],
                DefaultTarget::ProviderId("p-oc".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-sonnet-4".into(),
            }],
        );
        // 平台别名不在槽位表 → 命中前缀 → 直给槽位上游模型
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5")
                .expect("alias hit");
        assert_eq!(prov.id, "p-oc");
        assert_eq!(upstream.as_deref(), Some("space-bunny-free"));
    }

    #[tokio::test]
    async fn alias_matching_is_case_insensitive() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-oc".to_string(),
            "OpenCode".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-oc",
                    AggregateTier::Sonnet,
                    "space-bunny-free",
                )],
                DefaultTarget::ProviderId("p-oc".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-sonnet-4".into(),
            }],
        );
        let (_, upstream) = resolve_target(&db, "claude-desktop", &aggregate, "CLAUDE-SONNET-5-5")
            .expect("case-insensitive alias hit");
        assert_eq!(upstream.as_deref(), Some("space-bunny-free"));
    }

    #[tokio::test]
    async fn alias_request_with_1m_suffix_still_matches() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-oc".to_string(),
            "OpenCode".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-oc",
                    AggregateTier::Sonnet,
                    "space-bunny-free",
                )],
                DefaultTarget::ProviderId("p-oc".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-sonnet-4".into(),
            }],
        );
        // [1m] 后缀在匹配前已被 strip_one_m_suffix_for_route_lookup 剥掉
        let (_, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5[1m]")
                .expect("alias hit after 1m strip");
        assert_eq!(upstream.as_deref(), Some("space-bunny-free"));
    }

    #[tokio::test]
    async fn alias_first_match_wins() {
        let db = crate::database::Database::memory().expect("db");
        for (id, name) in [("p-a", "A"), ("p-b", "B")] {
            let p = crate::provider::Provider::with_id(
                id.to_string(),
                name.to_string(),
                serde_json::json!({}),
                None,
            );
            db.save_provider("claude-desktop", &p).expect("save");
        }
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![
                    slot_for("claude-sonnet-4", "p-a", AggregateTier::Sonnet, "model-a"),
                    slot_for("claude-sonnet-2", "p-b", AggregateTier::Sonnet, "model-b"),
                ],
                DefaultTarget::ProviderId("p-b".into()),
            ),
            // 两条规则都命中 "claude-sonnet-5-5"：靠前者赢
            vec![
                AggregateAliasRule {
                    prefix: "claude-sonnet".into(),
                    slot_id: "claude-sonnet-4".into(),
                },
                AggregateAliasRule {
                    prefix: "claude-".into(),
                    slot_id: "claude-sonnet-2".into(),
                },
            ],
        );
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5")
                .expect("first rule wins");
        assert_eq!(prov.id, "p-a");
        assert_eq!(upstream.as_deref(), Some("model-a"));
    }

    #[tokio::test]
    async fn alias_dangling_slot_skipped_falls_to_default() {
        let db = crate::database::Database::memory().expect("db");
        let fallback = crate::provider::Provider::with_id(
            "p-fallback".to_string(),
            "Fallback".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &fallback).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-fallback",
                    AggregateTier::Sonnet,
                    "model-x",
                )],
                DefaultTarget::ProviderId("p-fallback".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-gone".into(),
            }],
        );
        // 悬空规则跳过 → 走兜底，且兜底不改写模型名（upstream = None）
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5")
                .expect("dangling alias must not error");
        assert_eq!(prov.id, "p-fallback");
        assert_eq!(upstream, None);
    }

    #[tokio::test]
    async fn alias_empty_or_whitespace_prefix_skipped() {
        let db = crate::database::Database::memory().expect("db");
        let fallback = crate::provider::Provider::with_id(
            "p-fallback".to_string(),
            "Fallback".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &fallback).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-fallback",
                    AggregateTier::Sonnet,
                    "model-x",
                )],
                DefaultTarget::ProviderId("p-fallback".into()),
            ),
            vec![
                AggregateAliasRule {
                    prefix: "".into(),
                    slot_id: "claude-sonnet-4".into(),
                },
                AggregateAliasRule {
                    prefix: "   ".into(),
                    slot_id: "claude-sonnet-4".into(),
                },
            ],
        );
        let (_, upstream) = resolve_target(&db, "claude-desktop", &aggregate, "anything")
            .expect("empty prefixes skipped");
        assert_eq!(upstream, None);
    }

    #[tokio::test]
    async fn exact_slot_match_wins_over_alias() {
        let db = crate::database::Database::memory().expect("db");
        for (id, name) in [("p-a", "A"), ("p-b", "B")] {
            let p = crate::provider::Provider::with_id(
                id.to_string(),
                name.to_string(),
                serde_json::json!({}),
                None,
            );
            db.save_provider("claude-desktop", &p).expect("save");
        }
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![
                    slot_for("claude-sonnet-4", "p-a", AggregateTier::Sonnet, "model-a"),
                    slot_for("claude-haiku-2", "p-b", AggregateTier::Sonnet, "model-b"),
                ],
                DefaultTarget::ProviderId("p-b".into()),
            ),
            // 前缀足以吞掉真槽 ID "claude-sonnet-4"，但精确命中优先
            vec![AggregateAliasRule {
                prefix: "claude-sonnet-4".into(),
                slot_id: "claude-haiku-2".into(),
            }],
        );
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-4")
                .expect("exact slot wins");
        assert_eq!(prov.id, "p-a");
        assert_eq!(upstream.as_deref(), Some("model-a"));
    }

    #[test]
    fn alias_rules_absent_in_old_json_deserializes_empty() {
        let json = r#"{"slots":[],"defaultTarget":{"kind":"providerId","value":"p"}}"#;
        let routes: AggregateRoutes = serde_json::from_str(json).unwrap();
        assert!(routes.alias_rules.is_empty());

        let with = r#"{"slots":[],"defaultTarget":{"kind":"providerId","value":"p"},"aliasRules":[{"prefix":"claude-sonnet","slotId":"claude-sonnet-4"}]}"#;
        let routes: AggregateRoutes = serde_json::from_str(with).unwrap();
        assert_eq!(routes.alias_rules.len(), 1);
        assert_eq!(routes.alias_rules[0].prefix, "claude-sonnet");
        assert_eq!(routes.alias_rules[0].slot_id, "claude-sonnet-4");
    }
```

- [ ] **Step 2: 跑测试确认编译失败（红）**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri" && export PATH="/c/Users/Jason/.cargo/bin:$PATH" && cargo test alias 2>&1 | tail -5
```

Expected: 编译错误（`AggregateAliasRule`/`alias_rules`/`with_alias_rules` 未定义）

- [ ] **Step 3: 实现**

`aggregate.rs`：

① 结构体（放在 `AggregateRouteSlot` 附近）：

```rust
/// 别名路由规则：请求名未命中任何槽位时，按前缀（大小写不敏感）转投目标槽位。
/// 按序先匹配先赢；空前缀/悬空槽位静默跳过（可选优化项失效，非配置性错误）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateAliasRule {
    pub prefix: String,
    pub slot_id: String,
}
```

② `AggregateRoutes` 加字段（`default_model` 之后）：

```rust
    #[serde(default)]
    pub alias_rules: Vec<AggregateAliasRule>,
```

③ `resolve_target` 在「未命中 → 默认目标」注释之前插入：

```rust
    // 别名层：槽位未命中时按序前缀匹配（小写化），命中且目标槽存在 → 视同命中该槽。
    // 悬空/空前缀静默跳过（debug 日志），不报错——别名是可选优化项，不是安全网。
    let lowered = requested.to_lowercase();
    for rule in &routes.alias_rules {
        let prefix = rule.prefix.trim().to_lowercase();
        if prefix.is_empty() || !lowered.starts_with(&prefix) {
            continue;
        }
        let Some(slot) = routes.slots.iter().find(|s| s.route_id == rule.slot_id) else {
            log::debug!(
                "[aggregate] alias rule '{prefix}' -> dangling slot '{}', skipped",
                rule.slot_id
            );
            continue;
        };
        let target = load_provider(db, app_type, &slot.provider_id)?;
        return Ok((target, Some(slot.upstream_model.clone())));
    }
```

④ 测试辅助（tests 模块内，`aggregate_with` 旁——注意它接收并返回 `Provider`，与测试里的 `with_alias_rules(aggregate_with(...), vec![...])` 用法一致）：

```rust
    /// 给聚合供应商设置别名规则（测试辅助）。`aggregate_with` 返回 `Provider`，
    /// 故本辅助同样进出 `Provider`。
    fn with_alias_rules(mut aggregate: Provider, rules: Vec<AggregateAliasRule>) -> Provider {
        aggregate
            .meta
            .as_mut()
            .and_then(|meta| meta.aggregate_routes.as_mut())
            .expect("aggregate routes")
            .alias_rules = rules;
        aggregate
    }
```

⑤ `grep -rn "AggregateRoutes {" src-tauri/src/`——所有字面量补 `alias_rules: vec![]`（含 `aggregate_with` 辅助与既有测试）。

- [ ] **Step 4: 跑测试确认绿**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri" && export PATH="/c/Users/Jason/.cargo/bin:$PATH" && cargo test aggregate 2>&1 | tail -5
```

Expected: aggregate 相关全 PASS（既有 + 新增 8 条）

- [ ] **Step 5: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src-tauri/src/aggregate.rs && git commit -m "feat(aggregate): 别名路由层——前缀规则（按序先赢、小写化、悬空跳过）"
```

---

### Task 2: TS 类型 + assignSlotIds 规则跟随（TDD）

**Files:**
- Modify: `src/types.ts`
- Modify: `src/utils/aggregateRoutes.ts`（`assignSlotIds`）
- Test: `src/utils/aggregateRoutes.test.ts`

**Interfaces:**
- Consumes: Task 1 的字段名 `aliasRules`（TS 侧 camelCase）/ `slotId`；现有 `assignSlotIds(routes) -> AggregateRoutes`
- Produces:
  - `export interface AggregateAliasRule { prefix: string; slotId: string }`
  - `AggregateRoutes.aliasRules?: AggregateAliasRule[]`
  - 语义：规则 slotId 引用的槽被重编号 → 跟随到新 ID；引用的槽已删 → slotId 置 `""`（行保留，运行时跳过）

- [ ] **Step 1: 写失败测试（aggregateRoutes.test.ts 末尾新增 describe）**

```ts
import type { AggregateAliasRule } from "@/types"; // 顶部 import 区补充

describe("assignSlotIds aliasRules 跟随", () => {
  const base = {
    slots: [
      { routeId: "claude-sonnet-5", tier: "sonnet" as const, providerId: "p1", upstreamModel: "m1", supports1m: false },
      { routeId: "claude-haiku-3", tier: "haiku" as const, providerId: "p1", upstreamModel: "m2", supports1m: false },
    ],
    defaultTarget: { kind: "providerId" as const, value: "p1" },
  };

  it("引用的槽被重编号时规则跟随到新 ID", () => {
    const routes = { ...base, aliasRules: [{ prefix: "claude-haiku", slotId: "claude-haiku-3" }] };
    // 在最前插入一个 haiku 槽：新槽拿池首 claude-haiku-4-5，原 haiku-3 槽序号 2 溢出为 claude-haiku-2
    const inserted = {
      ...routes,
      slots: [
        { routeId: "", tier: "haiku" as const, providerId: "p1", upstreamModel: "m0", supports1m: false },
        ...routes.slots,
      ],
    };
    const out = assignSlotIds(inserted);
    expect(out.slots[2].routeId).toBe("claude-haiku-2");
    expect(out.aliasRules?.[0]).toEqual({ prefix: "claude-haiku", slotId: "claude-haiku-2" });
  });

  it("引用的槽已删时 slotId 清空、行保留", () => {
    const routes = {
      ...base,
      slots: base.slots.slice(0, 1), // 删掉 haiku 槽
      aliasRules: [{ prefix: "claude-haiku", slotId: "claude-haiku-3" }],
    };
    const out = assignSlotIds(routes);
    expect(out.aliasRules).toEqual([{ prefix: "claude-haiku", slotId: "" }]);
  });

  it("无 aliasRules 的旧数据往返后仍为 undefined", () => {
    expect(assignSlotIds(base).aliasRules).toBeUndefined();
  });
});
```

- [ ] **Step 2: 红验证**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/aggregateRoutes.test.ts 2>&1 | tail -5
```

Expected: FAIL（类型不存在 / 跟随未实现）

- [ ] **Step 3: 实现**

`src/types.ts`（`AggregateRouteSlot` 附近）：

```ts
export interface AggregateAliasRule {
  prefix: string;
  /** 目标槽位 routeId；assignSlotIds 重编号时跟随，悬空置空串 */
  slotId: string;
}
```

`AggregateRoutes` 接口加：

```ts
  /** 别名路由规则：槽位未命中时按前缀（小写化、按序先赢）转投。缺省 = 无规则。 */
  aliasRules?: AggregateAliasRule[];
```

`src/utils/aggregateRoutes.ts` 的 `assignSlotIds`，在 `defaultModel` 跟随段之后、`return` 之前加：

```ts
  const aliasRules = routes.aliasRules?.map((rule) => {
    const index = routes.slots.findIndex((slot) => slot.routeId === rule.slotId);
    return index >= 0 ? { ...rule, slotId: ids[index] } : { ...rule, slotId: "" };
  });
```

并把返回对象的属性改为 `...(aliasRules ? { aliasRules } : {})`：

```ts
  return { ...routes, slots, defaultTarget, defaultModel, ...(aliasRules ? { aliasRules } : {}) };
```

（原 return 里 `defaultModel` 已在——按文件现状最小改动合入，勿重复展开既有字段。）

- [ ] **Step 4: 绿验证 + typecheck**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/aggregateRoutes.test.ts 2>&1 | tail -4 && npx tsc --noEmit 2>&1 | tail -2; echo "exit: $?"
```

Expected: 全 PASS；typecheck exit 0

- [ ] **Step 5: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/types.ts src/utils/aggregateRoutes.ts src/utils/aggregateRoutes.test.ts && git commit -m "feat(aggregate): aliasRules 类型与 assignSlotIds 重编号跟随/悬空清空"
```

---

### Task 3: 编辑器「别名路由」区块 + i18n + 组件测试

**Files:**
- Modify: `src/components/providers/forms/AggregateProviderFields.tsx`
- Modify: `src/i18n/locales/zh.json`、`src/i18n/locales/en.json`（`aggregate` 段）
- Test: `tests/components/AggregateProviderFields.aliasRules.test.tsx`（新建）

**Interfaces:**
- Consumes: `AggregateAliasRule`（Task 2）、`commit(next: AggregateRoutes)`（本文件既有——内部已跑 assignSlotIds + maxEffort 归一化，规则 slotId 在其中跟随）、槽位下拉的既有格式「`labelOf(slot)` + ` (${routeId})`」
- Produces: 「别名路由」区块（增/删/改规则行）；运行行为由 Task 1/2 定义

- [ ] **Step 1: i18n 键（zh.json / en.json 的 aggregate 段，紧挨 maxEffortOff）**

zh：

```json
"aliasRouting": "别名路由（槽位未命中时按前缀转投，先匹配先赢）",
"aliasPrefix": "请求名前缀",
"aliasPrefixPlaceholder": "如 claude-sonnet",
"aliasTarget": "目标槽位",
"aliasTargetPlaceholder": "选择槽位",
"addAliasRule": "添加规则",
"removeAliasRule": "删除该规则",
"aliasTip": "前缀过宽（如 claude-）会吞掉所有未命中槽位的请求名，注意范围",
```

en：

```json
"aliasRouting": "Alias routing (prefix-based redirect when no slot matches, first match wins)",
"aliasPrefix": "Request prefix",
"aliasPrefixPlaceholder": "e.g. claude-sonnet",
"aliasTarget": "Target slot",
"aliasTargetPlaceholder": "Pick a slot",
"addAliasRule": "Add rule",
"removeAliasRule": "Remove rule",
"aliasTip": "An overly broad prefix (e.g. claude-) swallows every unmatched name; mind the scope",
```

- [ ] **Step 2: 写失败组件测试**

```tsx
// tests/components/AggregateProviderFields.aliasRules.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AggregateProviderFields } from "@/components/providers/forms/AggregateProviderFields";
import type { AggregateRoutes, Provider } from "@/types";

const value: AggregateRoutes = {
  slots: [
    { routeId: "claude-sonnet-4", tier: "sonnet", providerId: "p1", upstreamModel: "space-bunny-free", label: "OC · space-bunny-free", supports1m: false },
  ],
  defaultTarget: { kind: "providerId", value: "p1" },
  aliasRules: [
    { prefix: "claude-sonnet", slotId: "claude-sonnet-4" },
    { prefix: "claude-", slotId: "" },
  ],
};

const props = {
  value,
  onChange: vi.fn(),
  candidates: [{ id: "p1", name: "OC" } as unknown as Provider],
  modelsForProvider: () => [],
  fetchingProviderId: null,
  onFetchModels: vi.fn(),
};

describe("别名路由区块", () => {
  it("渲染既有规则行（含悬空行），悬空行显示占位而非崩", () => {
    render(<AggregateProviderFields {...props} />);
    const prefixInput = screen.getByDisplayValue("claude-sonnet");
    expect(prefixInput).toBeInTheDocument();
    // 第二行 slotId 为空 → Select 显示占位（存在第二个 combobox 即可）
    expect(screen.getAllByRole("combobox").length).toBeGreaterThanOrEqual(2);
  });

  it("添加规则产生空行并回调 onChange", async () => {
    const user = userEvent.setup();
    render(<AggregateProviderFields {...props} />);
    await user.click(screen.getByRole("button", { name: "添加规则" }));
    expect(props.onChange).toHaveBeenCalled();
    const calls = props.onChange.mock.calls;
    const last = calls[calls.length - 1][0] as AggregateRoutes;
    expect(last.aliasRules).toHaveLength(3);
    expect(last.aliasRules?.[2]).toEqual({ prefix: "", slotId: "" });
  });

  it("删除规则行回调 onChange 且不残留", async () => {
    const user = userEvent.setup();
    render(<AggregateProviderFields {...props} />);
    const dels = screen.getAllByRole("button", { name: "删除该规则" });
    await user.click(dels[0]);
    const calls = props.onChange.mock.calls;
    const last = calls[calls.length - 1][0] as AggregateRoutes;
    expect(last.aliasRules).toEqual([{ prefix: "claude-", slotId: "" }]);
  });
});
```

（选择器提示：`name: "添加规则"` 依赖 t() 的 defaultValue 中文——组件代码里必须带；若 aria-name 断言不稳，fallback 用 `title` 属性定位删除钮。）

- [ ] **Step 3: 红验证**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run tests/components/AggregateProviderFields.aliasRules.test.tsx 2>&1 | tail -5
```

Expected: FAIL（区块不存在）

- [ ] **Step 4: 组件实现（AggregateProviderFields.tsx）**

① import 补 `AggregateAliasRule`（并入 `@/types` 既有 type import）。

② 处理函数（`removeCard` 附近）：

```tsx
  /** 别名规则改动直接走 commit：assignSlotIds 会跟随重编号，maxEffort 归一化不受影响。 */
  const patchAliasRule = (index: number, patch: Partial<AggregateAliasRule>) => {
    const next = [...(value.aliasRules ?? [])];
    next[index] = { ...next[index], ...patch };
    commit({ ...value, aliasRules: next });
  };
  const addAliasRule = () =>
    commit({ ...value, aliasRules: [...(value.aliasRules ?? []), { prefix: "", slotId: "" }] });
  const removeAliasRule = (index: number) =>
    commit({ ...value, aliasRules: (value.aliasRules ?? []).filter((_, i) => i !== index) });
```

③ 区块 JSX：放在「默认模型」下拉行之后、DndContext 之前（与兜底目标同层）：

```tsx
<div className="space-y-2 rounded-md border border-border-default p-3">
  <p className="text-xs text-muted-foreground" title={t("aggregate.aliasTip", { defaultValue: "前缀过宽（如 claude-）会吞掉所有未命中槽位的请求名，注意范围" })}>
    {t("aggregate.aliasRouting", { defaultValue: "别名路由（槽位未命中时按前缀转投，先匹配先赢）" })}
  </p>
  {(value.aliasRules ?? []).map((rule, index) => (
    <div key={index} className="flex items-center gap-2">
      <Input
        className="h-8 w-56 shrink-0"
        value={rule.prefix}
        placeholder={t("aggregate.aliasPrefixPlaceholder", { defaultValue: "如 claude-sonnet" })}
        title={t("aggregate.aliasPrefix", { defaultValue: "请求名前缀" })}
        onChange={(e) => patchAliasRule(index, { prefix: e.target.value })}
      />
      <span className="text-xs text-muted-foreground">→</span>
      <Select
        value={rule.slotId || UNSET}
        onValueChange={(v) => patchAliasRule(index, { slotId: v === UNSET ? "" : v })}
      >
        <SelectTrigger className="h-8 flex-1">
          <SelectValue placeholder={t("aggregate.aliasTargetPlaceholder", { defaultValue: "选择槽位" })} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={UNSET}>{t("aggregate.aliasTargetPlaceholder", { defaultValue: "选择槽位" })}</SelectItem>
          {value.slots.map((slot) => (
            <SelectItem key={slot.routeId} value={slot.routeId}>
              {labelOf(slot)} ({slot.routeId})
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="h-7 w-6 shrink-0 text-muted-foreground hover:text-destructive"
        title={t("aggregate.removeAliasRule", { defaultValue: "删除该规则" })}
        onClick={() => removeAliasRule(index)}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  ))}
  <Button
    type="button"
    variant="ghost"
    size="sm"
    className="h-7 gap-1 text-xs text-muted-foreground"
    onClick={addAliasRule}
  >
    <Plus className="h-3.5 w-3.5" />
    {t("aggregate.addAliasRule", { defaultValue: "添加规则" })}
  </Button>
</div>
```

（`Input`/`Select*`/`Button`/`X`/`Plus` 均为文件既有 import；`UNSET` 为文件顶部既有常量 `"__unset__"`——Radix Select 不接受空字符串 value，空 slotId 用 UNSET 占位。）

- [ ] **Step 5: 绿验证 + typecheck + 全量前端**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run tests/components/AggregateProviderFields.aliasRules.test.tsx 2>&1 | tail -4 && npx tsc --noEmit 2>&1 | tail -2 && npx vitest run 2>&1 | tail -4
```

Expected: 新用例 PASS；typecheck 0；全量仅已知并发 flaky（PiProviderForm/App 集成——单跑通过即可）

- [ ] **Step 6: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/components/providers/forms/AggregateProviderFields.tsx src/i18n/locales/zh.json src/i18n/locales/en.json tests/components/AggregateProviderFields.aliasRules.test.tsx && git commit -m "feat(aggregate): 编辑器别名路由区块——前缀+目标槽位行，不完整行保留"
```

---

### Task 4: 构建部署 + 实机验收 + 推送

**Files:**
- 无代码改动；DB 配置 + 部署 + 验收

- [ ] **Step 1: 构建（后台，约 15 分钟）**

```bash
cd "D:/Workspace/Project/cc-switch/src" && export PATH="/c/Users/Jason/.cargo/bin:$PATH" && export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR="https://gh-proxy.com/https://github.com/" && unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy && pnpm tauri build --bundles nsis 2>&1 | tail -5
```

Expected: exit 0，产物 `src-tauri/target/release/bundle/nsis/CC Switch_3.20.4-local_x64-setup.exe`

- [ ] **Step 2: 停进程 → 安装 → 三重校验 → 启动**

```bash
MSYS_NO_PATHCONV=1 taskkill /F /IM cc-switch.exe > /dev/null 2>&1; for i in $(seq 1 10); do tasklist 2>/dev/null | grep -qi "cc-switch.exe" || { echo "已退出"; break; }; sleep 2; done; MSYS_NO_PATHCONV=1 cmd /c "D:\Workspace\Project\cc-switch\tools\install-local.bat" 2>&1 | tail -3
```

```bash
cd "D:/Workspace/Project/cc-switch/src" && md5sum src-tauri/target/release/cc-switch.exe "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" && grep -a -c "$(ls dist/assets/ | grep '^index-.*\.js$')" "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" && grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6" "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" || true
```

Expected: 两个 md5 一致；资源名命中 1；pubkey 计数 0

```bash
cmd //c start "" "C:\Users\Jason\AppData\Local\Programs\CC Switch\cc-switch.exe" && sleep 12 && tasklist 2>/dev/null | grep -i "cc-switch.exe" | head -1
```

- [ ] **Step 3: DB 直写两条真实规则 + 重启**

```bash
python << 'PYEOF'
import sqlite3, json
db = r'C:\Users\Jason\.cc-switch\cc-switch.db'
c = sqlite3.connect(db)
meta = json.loads(c.execute("select meta from providers where rowid=57").fetchone()[0])
agg = meta['aggregateRoutes']
ids = [s['routeId'] for s in agg['slots']]
assert 'claude-sonnet-4' in ids and 'claude-haiku-3' in ids, ids
agg['aliasRules'] = [
    {"prefix": "claude-sonnet", "slotId": "claude-sonnet-4"},
    {"prefix": "claude-haiku", "slotId": "claude-haiku-3"},
]
c.execute("update providers set meta=? where rowid=57", (json.dumps(meta, ensure_ascii=False),))
c.commit()
print('aliasRules 已写入:', json.dumps(agg['aliasRules'], ensure_ascii=False))
PYEOF
```

然后重启 CC Switch（命令同 Step 2 的停/启两条，注意启动后 sleep 12——代理就绪比进程出现晚几秒，连接被拒不代表失败，先查 `netstat -ano | grep 15721`）。

- [ ] **Step 4: 实机验收**

```bash
python << 'PYEOF'
import json, os, urllib.request, urllib.error, time
PROFILE = os.path.expandvars(r'%LOCALAPPDATA%\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json')
BASE = "http://127.0.0.1:15721/claude-desktop"
token = json.load(open(PROFILE, encoding='utf-8'))['inferenceGatewayApiKey']
def call(model):
    body = json.dumps({"model": model, "max_tokens": 8, "messages": [{"role":"user","content":"ping"}]}).encode()
    req = urllib.request.Request(BASE + "/v1/messages", data=body, method="POST",
        headers={"content-type":"application/json","authorization":f"Bearer {token}","x-api-key":token,"anthropic-version":"2023-06-01"})
    t=time.time()
    try:
        with urllib.request.urlopen(req, timeout=60) as r: return r.status, round(time.time()-t,1)
    except urllib.error.HTTPError as e: return e.code, round(time.time()-t,1)
    except Exception as e: return -1, str(e)[:40]
print("别名落点：")
for m in ["claude-sonnet-5-5", "CLAUDE-SONNET-5-5", "claude-haiku-4-5-20251001", "zzz-unmatched-name"]:
    st, el = call(m)
    print(f"  {m:28s} -> {st}  {el}s")
import sqlite3, datetime
c = sqlite3.connect(r'C:\Users\Jason\.cc-switch\cc-switch.db')
names = {r[0]: r[1] for r in c.execute("select id, name from providers")}
print("\n落点归属（最近 4 条）：")
for r in c.execute("select created_at, request_model, model, provider_id from proxy_request_logs order by created_at desc limit 4"):
    print(f"  {datetime.datetime.fromtimestamp(r[0]).strftime('%H:%M:%S')}  {r[1]:26s} -> {r[2]:24s} {names.get(r[3],'?')}")
PYEOF
```

Expected（落点归属为准，非仅状态码）：
- `claude-sonnet-5-5` / `CLAUDE-SONNET-5-5` → `space-bunny-free`（OpenCode Go）
- `claude-haiku-4-5-20251001` → `longcat-2.5-preview-free`（OpenCode Go）
- `zzz-unmatched-name` → 400（兜底路径经目标供应商自身映射表，未知 shape 显式报错，既有设计）

- [ ] **Step 5: 15 槽回归**（复用既有脚本：profile inferenceModels 逐槽真实请求，断言 15/15 全 200）

- [ ] **Step 6: 账本 + 提交推送**

`.superpowers/sdd/progress.md` 追加（别名列落地、验收落点、回归结果）；快照入库并推送：

```bash
cd "D:/Workspace/Project/cc-switch/src" && cp .superpowers/sdd/progress.md docs/local-maintenance/progress.md && git add docs/local-maintenance/progress.md && git commit -m "docs: 账本——别名路由落地与实机验收" && git push origin fix/profile-merge
```

（推送连接重置时 fallback：`git -c http.https://github.com.proxy=http://127.0.0.1:9674 push origin fix/profile-merge`）
