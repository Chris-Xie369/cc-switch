# 聚合别名路由：前缀规则层（2026-09-30）

## 背景与动机

2026-09-29/30 实测发现：Claude Code 的平台别名（`sonnet`→`claude-sonnet-5-5`、
`haiku`→`claude-haiku-4-5-20251001`、`fable`→`claude-fable-4`）经网关时不在槽位表
内——落兜底（一度是付费 DeepSeek，433 条 $0.54）或撞真槽位（fable-4=k3，
controller 误派 ≈$5）。止血（兜底改 space-bunny 免费槽）已做，但兜底语义不可控：

- 兜底不改写模型名，目标供应商**自己的路由表**会再解释一次（实测
  `claude-haiku-4-5-20251001` 被 OpenCode 路由表映射到 deepseek-flash，落点漂移）
- 撞真槽位（fable-4）无法用兜底解决

根治 = 在聚合路由加一层**用户可配的别名规则**：请求名未命中槽位时，按前缀匹配
转投指定槽位（复用槽位的供应商 + 上游模型，与精确命中同路径）。

## 已确认决策（brainstorming 对齐）

1. **前缀匹配**（非精确名）：一条 `claude-sonnet` 覆盖 `claude-sonnet-5-5` 及未来
   一切变体（平台改名/加日期后缀不失效）
2. **规则顺序不可调**：按添加顺序先匹配先赢；顺序不对删了重加。规则量级 ≤5 条

## 数据模型

```ts
// types.ts
export interface AggregateAliasRule {
  prefix: string;
  /** 目标槽位 routeId；跟随 assignSlotIds 重编号 */
  slotId: string;
}
// AggregateRoutes 增补：
export interface AggregateRoutes {
  slots: AggregateRouteSlot[];
  defaultTarget: DefaultTarget;
  defaultModel?: string;
  /** 别名路由规则（按序前缀匹配，先匹配先赢）。缺省 = 无规则。 */
  aliasRules?: AggregateAliasRule[];
}
```

Rust（`aggregate.rs`）：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateAliasRule {
    pub prefix: String,
    pub slot_id: String,
}
// AggregateRoutes 增补（零迁移，同 maxEffort 先例）：
#[serde(default)]
pub alias_rules: Vec<AggregateAliasRule>,
```

存量字面量构造处补 `alias_rules: vec![]`（实测 22 处：proxy/server.rs、
services/provider/mod.rs、services/stream_check.rs、aggregate.rs 测试、
claude_desktop_config.rs 测试）。

## 路由语义（resolve_target 分层）

```
请求名 → strip_one_m（既有）→ 小写化
  ① 精确槽位 routeId 命中（既有）——别名永不遮蔽真槽位
  ② 【新增】alias_rules 按序：requested.starts_with(rule.prefix)（前缀也小写化）
       首个命中且 rule.slot_id 能解析到槽位 → 该槽位的 (供应商, 上游模型)
       （与精确命中同路径：上游模型直给，不经目标供应商自己的路由表）
  ③ 未命中 → 兜底目标（既有语义不动）
```

运行时容忍（静默跳过，不报错）：空前缀；slot_id 解析不到槽位（悬空）。仅日志
**info** 一条（2026-09-30 执行中修正：终审指出 debug 在默认日志档位不可见，
而这是用户发现「别名规则悬空」的唯一自助线索；半成品规则 slot_id 为空时不记，
避免编辑期间每次请求刷行）。**错误分级对齐既有约定**：兜底槽缺失=显式报错（配置性错误），
悬空别名=静默（可选优化项失效）。

小写化匹配：请求名与前缀均 `to_lowercase()` 后前缀比较。

## 编辑器 UI（AggregateProviderFields）

「兜底目标 / 默认模型」行下方新增**别名路由**区块：

```
别名路由（槽位未命中时按前缀转投，先匹配先赢）
[前缀 Input          ] → [OpenCode Go · space-bunny (claude-sonnet-4) ▾] [×]
[+ 添加规则]
```

- 目标槽位下拉 = 全部槽位，格式与「默认模型」下拉一致（`显示名 (routeId)`）
- 规则顺序 = 添加顺序，无排序控件（已确认）
- 不完整规则（前缀空或槽位未选）**原样保留行**——运行时跳过（编辑宽容、运行容忍）
- `assignSlotIds` 重编号跟随：能解析的 rule.slotId 映射到新 ID；指向已删槽的 →
  slotId 置空（行保留，防幽灵重挂到未来同 ID 槽；同 defaultModel 悬空清空先例）
- i18n zh/en 各 ~6 键（aliasRouting / aliasPrefix / aliasTarget / addAlias /
  removeAlias / aliasTip）

## 明确不做（边界）

- 别名只影响**网关路由**，不进 profile / inferenceModels（选择器不可见）
- 不支持精确名规则、正则、通配符（前缀已覆盖）
- 不为 `claude-fable-4`（真槽 ID）提供任何旁路——精确命中优先是安全属性
- 不做规则校验拦截（重前缀如 `claude-` 吞一切未命中名：用户自担，tooltip 提示即可）

## 测试

- Rust（resolve_target）：
  - 前缀命中转投槽位（含上游模型直给、不经目标供应商路由表）
  - 大小写不敏感（`CLAUDE-SONNET-5-5` 命中 `claude-sonnet` 前缀）
  - 多规则按序先赢
  - 悬空 slot_id 跳过、空前缀跳过、均未命中走兜底
  - 真槽位优先于别名（构造前缀能吞掉真槽 ID 的规则，请求真槽 ID 仍精确命中）
  - serde：旧 JSON（无 aliasRules）反序列化为空 vec
- TS（aggregateRoutes.test.ts）：assignSlotIds 对规则的跟随/清空
- 组件测试：加行/选槽/删行、不完整行保留
- 实机验收：配 `claude-sonnet`→space-bunny、`claude-haiku`→longcat 两条真实规则，
  发 `claude-sonnet-5-5`、`claude-haiku-4-5-20251001` 验证落点；15 槽回归

## 实施顺序

```
1. Rust 数据结构 + resolve_target 别名层 + 测试（TDD）
2. TS 类型 + assignSlotIds 跟随 + 测试
3. 编辑器 UI + i18n + 组件测试
4. 字面量补齐 + cargo test + vitest + typecheck 全绿
5. 构建部署（UPSTREAM-SYNC §5–§6）+ 实机验收 + 账本
```

## 风险

| 风险 | 对策 |
|---|---|
| 过宽前缀（如 `claude-`）吞掉所有漏网名 | tooltip 提示；运行时行为可从代理日志复核 |
| 规则槽 ID 轮换后悬空 | assignSlotIds 跟随已处理；残余悬空静默跳过有日志 |
| 字面量漏补编译失败 | cargo check 兜底（编译器强制） |
