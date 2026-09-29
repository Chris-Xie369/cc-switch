# 聚合编辑器「诚实化」：三态强度徽标 + 池连续化 + maxEffort 入 UI（2026-09-29）

## 背景与目标

MoA 聚合功能要走「分享给朋友（Windows）→ 上游 PR」的路径，评估结论：
数据模型与 UI 本就通用，真正的债是三处——

1. **配置后果不可见**（最大断点）：槽位有没有思考档位、1M 是否真支持、延迟多高，
   全部是编辑器看不见的隐式规则，错误延迟到运行期以无关症状爆发。
2. **私人历史入代码**：sonnet 池 `{1:4-6, 2:4-5, 4:5}` 的 3 位留空是为保住本机存量
   `claude-sonnet-3` 的定制，对全新用户分配次序反直觉（第 3 槽无控件、第 4 槽反而有）。
3. **maxEffort 脚本直写**：UI 保存会抹掉 profile 里的 `maxEffort`（v3 spec 已记录的缺口），
   朋友没有配套脚本。

本 spec 一次清掉三处。**明确不做**：1M 预检、延迟预检（独立第二 spec）、
per-provider 怪癖配置化、跨平台验证、上游 PR（阶段 2）。

> 依据（asar 2.9939.4.0，`app.asar` 内 `czt`/`lzt`/`szt`，grep 手册见 README）：
> 强度判定是三态而非二态——精确表 8 个 ID 之外，fable/mythos 族正则
> `/^(?:claude-)?(?:fable|mythos)(?:-|$)/` 兜底**完整阶梯**；`claude-sonnet-4-5`
> 与 `claude-haiku-4-5` 只有**扩展思考开关**（`modes:["extended"]`，无档位）；
> 其余（全部溢出 ID）**什么都没有**。`claude-sonnet-5` 在表内且阶梯最全
> （low…xhigh…max，推荐 medium），`claude-opus-5` 有 `disallowThinkingDisabled`（不收池）。

## 交付项

### 1. sonnet 池连续化 + 本机迁移（先做，徽标显示迁移后的真相）

- 池改 `{1: "claude-sonnet-4-6", 2: "claude-sonnet-4-5", 3: "claude-sonnet-5"}`，
  删除留空位注释。溢出跳过逻辑（撞池名顺移、assignSlotIds 批次 taken 集合）**不变**。
- 本机迁移（三处同步，参照 2026-09-29 kimi 槽位改档的既有手法）：
  - DB `providers.meta`（rowid 57）槽位 OC space-bunny：`tier=sonnet, routeId
    claude-sonnet-3 → claude-sonnet-5`
  - DB `defaultModel: "claude-sonnet-3" → "claude-sonnet-5"`（漏改会悬空回落排序首位）
  - profile `inferenceModels` 该条 `name` 改名（位置不动，改前备份）
- 迁移后重启 CC Switch，**15 槽回归全 200**（含 `claude-sonnet-5`）。
- 已知代价：Claude Desktop 记住的 `claude-sonnet-3` 选择失效回落默认（一次性）。
- 附带收益：默认模型（免费 space-bunny）从无控件升级为完整强度阶梯。

### 2. 三态思考档位徽标

- 新增 `src/utils/claudeDesktopCapability.ts`：
  `effortCapability(routeId): "ladder" | "extended" | "none"`——先剥 `[1m]` 后缀
  （大小写不敏感、允许尾随空白，与 asar `eC` 一致）再查表；表内含 8 个精确 ID +
  族正则；未知 ID → `none`。注释附 asar 验证 grep 手册，与 README 互引。
- 编辑器（AggregateProviderFields）槽位行档位名旁加徽标：
  `强度✓`（ladder，主色）/`思考开关`（extended，次级）/`✗`（none，暗灰），
  悬停 tooltip 说明原因（例：「池满溢出 ID，无思考控件」）。
- 折叠态摘要升级为 `N 个模型 · M 强度`（M = ladder 数）。
- 测试：三态 + 未知 ID 兜底 + `[1m]`/空白剥离 + fable 族正则。

### 3. maxEffort 进 UI

- `AggregateRouteSlot` 加 `maxEffort?: "low"|"medium"|"high"|"xhigh"|"max"`；
  Rust `AggregateRouteSlot` 加 `#[serde(default)] pub max_effort: Option<String>`，
  写 profile 时只接受枚举值（未知值丢弃）。`assignSlotIds` 展开保留该字段，零迁移。
- 编辑器槽位行加紧凑「强度上限」下拉：**仅当徽标 = ladder 时可选**（与徽标联动，
  extended/none 槽位显示为不可用），未设置 = 空选项。
- profile 写入：`inferenceModels` 条目带 `maxEffort`（形状以实施时 asar 复核为准；
  既有证据：per-entry `maxEffort` 参与 1M 变体折叠取最值）。
- 行为变化（已知并接受）：过去由脚本直写 profile 的 `maxEffort` 改由 UI 拥有，
  保存以槽位数据为准；旧值需在 UI 里重录一次。

### 4. README + 提交推送

- 先把存量未提交工作分批提交推送 origin（dropdown z-fix+测试、池 keyed 重构+测试、
  账本；**绝不推 upstream**，署名行照旧不加）。
- 新增 `src/README.md`（fork 根，面向外来者）：
  这是什么/为什么 → Windows 构建步骤（gh-proxy 镜像 + 代理绕行）→ 部署与校验
  （install-local.bat、md5 口径）→ **已知耦合清单**（Claude Desktop 版本依赖 +
  asar 验证手册 + 厂商词黑名单 + 三态强度表）→ 供应商怪癖清单（OpenCode
  session 头注入）→ 验收口径（槽位回归）。
- UPSTREAM-SYNC.md 保持本地视角不动。

## 实施顺序

```
1. 池连续化 + 本机迁移（先做，徽标显示迁移后的真相）   → 验证: 15 槽回归 200
2. 三态徽标（依赖 1 的 ID 分配结果）                   → 验证: 单测三态 + 实机徽标
3. maxEffort（依赖 2 的三态判定做联动）                → 验证: serde/单测 + profile 断言
4. README + 分批提交推送（收尾）                       → 验证: origin 与本地一致
```

## 测试与验收口径

- TS：slotId 新序（3→sonnet-5、4→sonnet-4、5→顺移 6）；effortCapability 三态/剥离/兜底；
  maxEffort 字段往返。
- Rust：serde 兼容（无 maxEffort 旧 JSON → None）；profile 写入含/不含 maxEffort 断言。
- 实机：构建部署 → 15 槽回归全 200 → 编辑器徽标与 asar 表抽查一致（含 4-6 无 xhigh、
  sonnet-4-5/haiku-4-5 仅开关两条易错点）→ maxEffort 设置后保存→重启→profile 存活。

## 风险

| 风险 | 对策 |
|---|---|
| Claude Desktop 升级漂移强度表 | 验证手册进 README；徽标表集中在单文件，重验成本低 |
| maxEffort 的 profile 字段形状有出入 | 实施时以 asar 复核为准，写入前单测锁形状 |
| 迁移改 ID 造成选择器记忆回落 | 一次性、已知；账本记录 |
