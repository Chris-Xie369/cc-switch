# Claude Desktop 多供应商共存（聚合供应商）设计

- 日期：2026-09-22
- 状态：设计已获用户逐节确认（4 节）
- 目标：让 Claude Desktop 在一次重启后，能在模型选择器里**自由选用多家供应商的模型**，无需再回 CC Switch 切换并重启

---

## 1. 需求

**现状痛点**：CC Switch 每次只能"启用"一个供应商；换一家必须回 CC Switch 切换 + 重启 Claude Desktop。

**用户诉求**（已确认）：一次配置多家 → 重启一次 → 之后在 Claude 的模型列表里**自由选择**任意一家的模型，选择权完全在用户。不需要按项目自动绑定这类自动化。

**路线图**：CC Switch 是通用模型配置工具，路线为 Claude Desktop → Claude Code CLI → 其他 Agent。本设计只覆盖 **Claude Desktop**，但路由层设计可复用于后续阶段。

---

## 2. 调研结论：社区已有实现，但不覆盖 Desktop

| 相关项 | 覆盖范围 | 结论 |
|---|---|---|
| **[#5937](https://github.com/farion1231/cc-switch/pull/5937)**（OPEN，+5195 行，CI 全绿，等 review 近两月） | **Claude Code CLI + Codex** 的聚合路由 | ❌ 不涉及 Claude Desktop（全量 diff 中 `claude_desktop`/`inferenceModels` 仅出现 2 次且无关） |
| [#5584](https://github.com/farion1231/cc-switch/pull/5584) | 被 #5937 取代 | 已关闭 |
| [#3703](https://github.com/farion1231/cc-switch/issues/3703)（18 评论，"血书"） | 同一诉求的社区聚集地 | 未实现 |
| [#7450](https://github.com/farion1231/cc-switch/issues/7450)（Hermes） | Hermes 集成 | 同类问题、不同形态：Hermes **原生支持**多供应商并列，只是被 CC Switch 覆盖 |
| 搜 Claude Desktop + 聚合/多供应商的 PR | — | **无结果** |

**结论**：Desktop 侧无现成轮子，但 #5937 的**设计可直接借鉴**——它是本设计的主要参考。

### 借鉴 #5937 的设计要点

- **虚拟「聚合供应商」**：自身无端点无凭据，只存路由表；按模型把请求分流到常规供应商
- **复用而非重造**：目标供应商的认证、协议转换、模型映射、熔断、故障转移全部沿用
- **界面「当前供应商」不变**：始终是聚合供应商，不随请求来回切
- **刻意不做负载均衡**：同一 session 轮询不同上游会打碎 prompt cache 且输出不一致；负载拆分交给客户端选模型，按模型确定性路由
- **路由表挂在供应商上**：天然继承增删改校验、删除保护、切换守卫，避免全局配置与运行参数耦合

---

## 3. 平台约束（均已实测确认）

| 约束 | 内容 | 证据 |
|---|---|---|
| 模型列表是**平铺**的 | `inferenceModels` 每条仅 `name` / `labelOverride` / `supports1m`；无分组字段 | profile 结构 + 目录 schema |
| 模型 ID 须形如 `claude-{角色}-{标识}` | 角色限 `sonnet`/`opus`/`haiku`/`fable`；违规触发 fail-all 整组拒收 | CC Switch `is_claude_safe_model_id` + `app.asar` 逆向 |
| **自定义后缀被接受**（关键前提） | 实测 `claude-sonnet-probe` 正常显示于选择器，**无 fail-all** → 槽位数量无上限 | **2026-09-22 探针实测**（见 §10） |
| `supports1m: true` 会**双行渲染** | 每个模型在选择器里变成两行（本体 + "1M context window"）。实测声明 5 条 → 显示 10 项 | 同上 |
| 描述文字来自签名目录 | 选择器里那句 "For your toughest challenges" 来自 Anthropic 目录中对应**角色**的元数据，用户改不了 | 探针显示 "Most efficient for everyday tasks" |
| 只有**单个** gateway 端点 | profile 中 `inferenceGatewayBaseUrl` / `inferenceGatewayApiKey` / `inferenceProvider` 均为单数 | 代码与 profile 实读 |
| 用户的多家供应商**在 Desktop 层面全属于 `gateway`** | `provider_alias_targets` 按 infrastructure provider（first_party/bedrock/vertex/**gateway**）映射，用户供应商不在其中 | 目录解码 |

**由约束推出的结论**：供应商区分只能落在 **gateway 暴露的模型 ID 与显示名**上。

---

## 4. 方案：聚合供应商

```
                    ┌─ 聚合供应商（虚拟，无端点无凭据）────────────┐
                    │  路由表：槽位 → (目标供应商, 上游模型, 档位, 显示名) │
                    │  默认目标：未命中时兜底                      │
                    └────────────────────────────────────────┘
Claude Desktop ──model="claude-sonnet-glm"──▶ 本地代理
                                                │ 当前供应商是聚合供应商？
                                                │ 是 → 按 model 查路由表
                                                ▼
                                    目标供应商（智谱）
                                    → 用它自己的凭据 / 协议转换 /
                                      模型映射 / 熔断 / 故障转移转发
```

**关键简化（相对最初设想）**：不再需要把「启用」改成多选。**一个聚合供应商就等价于"多家同时在线"**，且它就是普通供应商的一种，沿用既有的增删改查与启用流程。这避免改动 CC Switch 的全局语义，不波及 Claude Code / Codex / Hermes 等其他功能。

---

## 5. 数据模型

挂在 `Provider.meta.aggregateRoutes`（形状对齐 #5937，便于日后合并上游）：

```rust
/// 一个槽位 = 一个可被 Claude 选择的模型
pub struct AggregateRouteSlot {
    /// 档位：决定选择器里的描述文字来自目录中哪个角色
    pub tier: AggregateTier,          // Sonnet | Opus | Haiku | Fable
    /// 目标供应商（被引用者受删除保护）
    pub provider_id: String,
    /// 该供应商的上游模型名
    pub upstream_model: String,
    /// 选择器显示名，如 "智谱 GLM-5.3"
    pub label: Option<String>,
    /// 是否勾选 1M 上下文（勾了会在选择器里多出一行）
    pub supports_1m: bool,
}

pub struct AggregateRoutes {
    pub slots: Vec<AggregateRouteSlot>,
    /// 未命中路由时的兜底目标。**按槽位 ID 引用**（不是下标——下标会随槽位增删重排而失效）；
    /// 也可直接指向某个供应商 id。
    pub default_target: DefaultTarget,   // SlotId(String) | ProviderId(String)
}
```

**槽位 ID 自动生成**（用户不手填）：`claude-{tier}-{slug}`

- `slug` 生成规则：取目标供应商名称 → 转小写 → 非字母数字字符替换为 `-` → 合并连续 `-` → 去除首尾 `-` → 截断到 20 字符；若结果为空则回落为供应商 id 的前 8 位
  - 例：`智谱 GLM` → 名称无 ASCII 字母 → 回落为供应商 id 前缀；`DeepSeek-OTN` → `deepseek-otn`
  - 中文名供应商一律走 id 回落（避免生成空 slug）
- 冲突时追加序号：`claude-sonnet-deepseek-otn`、`claude-sonnet-deepseek-otn-2`
- 生成的 ID 必须通过 `is_claude_safe_model_id` 校验（角色前缀 + 非空标识）

---

## 6. 请求路由流程

```
① 请求到达代理，body.model = 槽位 ID
② 取该 app 的当前供应商
   ├─ 非聚合供应商 → 走现有路径（**完全不变，零影响**）
   └─ 聚合供应商 → 查 slots
        ├─ 命中 → 取 (目标供应商, 上游模型)
        │        → 用目标供应商完成转发（认证 / 协议转换 / 模型映射 / 熔断 / 故障转移）
        │        → 上游模型名按槽位改写
        └─ 未命中 → 走 default_target
                     └─ 默认目标亦不可用 → 返回明确错误（不静默降级）
```

**熔断**：目标供应商的熔断器状态被直接复用——熔断中的目标视为不可用（不引入聚合层自己的故障转移策略，与 #5937 一致）。

---

## 7. Desktop 暴露（profile 写入）

启用聚合供应商时，沿用**已修复的 profile 写入路径**（补丁 A 的合并语义 + 补丁 B 的显示名注入）：

```
inferenceModels = slots.map(slot => ({
    name:           生成的槽位 ID,
    labelOverride:  用户填的显示名（缺省用上游模型名）,
    supports1m:     slot.supports_1m,   // 按需，勾了会双行
}))
```

其余键（网关地址、token、显示名三键等）全部走既有逻辑，**补丁 A/B 的语义不受影响**。

**生效时机**：profile 在 Claude Desktop 启动时读取 → 改路由表后需**重启 Claude Desktop**。CC Switch 在启用/保存聚合供应商后给出明确提示。

---

## 8. 配置界面

新增供应商类型「聚合供应商」，表单：

| 区域 | 内容 |
|---|---|
| 槽位列表 | 每行：`档位`（下拉，默认 sonnet）· `目标供应商`（下拉）· `上游模型`（从该供应商的模型列表选）· `显示名`（可选）· `1M`（勾选） |
| 默认目标 | 下拉选择某个槽位或某个供应商（未命中时的兜底） |
| 槽位 ID 预览 | 实时显示自动生成的 ID（只读），让用户知道 Claude 里会看到什么 |

**校验**：
- 至少一个槽位；`default_target` 必填
- 目标供应商不能是聚合供应商（禁嵌套）
- 被引用的供应商不可删除（删除保护）
- 生成 ID 必须通过 `is_claude_safe_model_id`

---

## 9. 边界与错误处理

| 情况 | 处理 |
|---|---|
| 目标供应商被删除 | 删除保护：被路由表引用的供应商不可删 |
| 聚合供应商互相引用（嵌套） | 禁止 |
| 两个槽生成同名 ID | 自动加序号去重 |
| 目标供应商熔断中 | 视为不可用（复用其熔断器状态） |
| 请求未命中路由 | 走 `default_target` |
| `default_target` 亦不可用 | 返回明确错误，不静默降级 |
| 切到官方供应商 | profile 被删（既有行为）；聚合路由表仍在，切回即恢复 |
| 空槽位 / 未配置默认目标 | 保存时校验拦截 |

---

## 10. 可行性验证记录（2026-09-22）

**目的**：确认「自定义后缀的模型 ID 是否被 Claude Desktop 接受」。这是本方案的承重假设——若不成立，槽位数量将被限制在目录内的 9 个官方 ID。

**做法**：向 profile 的 `inferenceModels` 追加一条探针（4 → 5 条）：

```json
{"labelOverride": "PROBE自定义ID", "name": "claude-sonnet-probe", "supports1m": true}
```

**结果**：重启 Claude Desktop 后，选择器显示 **10 项**——`PROBE自定义ID` 与 `PROBE自定义ID 1M` 均在列表中。

**结论**：
1. ✅ **自定义后缀 ID 被接受**，无 fail-all → 槽位数量无上限
2. 附带确认：`supports1m` 使每个模型**双行渲染**（5 条声明 → 10 项显示）
3. 备份：`pre-probe-backup-20260922_102856.json`；回滚脚本：`tools/revert-probe.bat`（本次未触发）

---

## 11. 测试策略

| 层次 | 内容 |
|---|---|
| 单元（Rust） | 槽位 ID 生成与去重；ID 合法性（复用 `is_claude_safe_model_id`）；路由表查询（命中 / 未命中 / 默认目标）；校验规则（禁嵌套、必填项） |
| 单元（Rust） | `inferenceModels` 由槽位生成（name / labelOverride / supports1m 三项正确） |
| 集成（Rust） | 代理路由：给定当前供应商为聚合 + 请求模型 → 选中正确的目标供应商（复用现有 `provider_router` 测试模式） |
| 手工验收 | 启用配置了多家的聚合供应商 → 重启 Claude → 选择器显示全部槽位 → 逐个选不同供应商的模型并确认请求确实打到对应的上游（看 CC Switch 日志的目标域名） |

（前端不做组件测试：本项目的既有惯例是前端以 `pnpm typecheck` + 手工验收把关。）

---

## 12. 与既有补丁的关系 / 上游化路径

- 本设计将成为**补丁 E**，与 A（合并语义）/ B（显示名可配置）/ C（本地构建）/ D（上游状态检查）并列，纳入 `UPSTREAM-SYNC.md` 的补丁清单
- **与 #5937 的关系**：其代理层路由机制（按模型选供应商）与本设计相同，差异仅在"如何把模型暴露给客户端"（CLI 用环境变量 / Desktop 用 `inferenceModels`）。两者**互补**——将来若要把 Desktop 支持上游化，可作为 #5937 的扩展提出
- **上游化可行性**：这是社区的普遍诉求（#3703 血书、#5109、#7146），#5937 已证明维护者不排斥该方向（只是迟迟未 review），因此本设计**有较大概率被上游接受**

---

## 13. 已知未决 / 后续

1. **槽位 ID 中的供应商短标识用名称还是数据库 id**：用名称可读但重命名会导致 ID 变化；用 id 稳定但不可读。本设计取**名称短标识**（可读、便于排查），并把"重命名后需重选模型"列为已知代价。若日后证明干扰大，可改为 id。
2. **不做负载均衡**：与 #5937 一致，同一模型永远命中同一上游。同模型多渠道均衡属 Key 池范畴，正交，留待后续。
3. **不覆盖 CLI / 其他 Agent**：路线图后续阶段另行设计；本设计的路由层应保持可复用。
