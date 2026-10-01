# Codex 聚合设计（Codex CLI 多供应商共存）

日期：2026-10-01 · 状态：设计定稿（自主会话执行，用户审批门后置到交付复核）
事实基础：`.superpowers/sdd/codex-aggregate-research.md`（14 条缺口清单，均带 文件:行 证据）
参照：Claude Desktop 聚合（spec `2026-09-22-claude-desktop-multi-provider-design.md`）与上游 PR #5937 的 Codex 侧

## 1. 目标

一个 Codex 聚合供应商（无端点无凭据）：槽位表把**客户端模型名**映射到目标 Codex
供应商 + 上游模型；Codex CLI 的 `/model` 选择器一次列出全部槽位模型（catalog 文件），
按模型分流经本地代理打到不同上游。与 Claude Desktop 聚合同一心智模型，独立实现。

非目标：别名规则、1M 变体、思考档位（Codex 有自己的 reasoning levels 概念）、
与上游 #5937 的形状兼容（见 D1）。

## 2. 设计决策

### D1. 数据模型：独立 meta 键 `codexAggregateRoutes`（不复用 `aggregateRoutes`）

```rust
// provider.rs ProviderMeta 新增
pub codex_aggregate_routes: Option<crate::aggregate::CodexAggregateRoutes>, // serde: codexAggregateRoutes

pub struct CodexAggregateRoutes {
    pub slots: Vec<CodexAggregateSlot>,   // 有序（catalog 顺序）
    pub default_target: DefaultTarget,    // 复用既有 DefaultTarget（providerId | slotId）
    pub default_model: Option<String>,    // 槽位 model；置顶 catalog 与顶层 model
}
pub struct CodexAggregateSlot {
    pub model: String,          // 客户端模型名（slug，路由键）；非空、全表唯一
    pub provider_id: String,    // 目标 Codex 供应商
    pub upstream_model: String, // 发往目标的模型名；非空
    pub label: Option<String>,  // catalog displayName（"Kimi · kimi-k2"）
}
```

- **理由**：Claude 侧 `route_id` 受 `is_claude_safe_model_id` 约束，与 Codex 自由模型名
  冲突；tier/maxEffort 是 Claude 概念；上游 #5937 占用 `aggregateRoutes` 键
  （4 档+custom map 形状互斥）——独立键名让未来同步**不新增碰撞**。
- 查表语义：`model` 精确匹配（大小写敏感，同 #5937）；未命中走 `default_target`
  （模型名不改写，与 Claude 侧一致）；无别名层（YAGNI）。

### D2. 客户端可见列表 = 合成 `modelCatalog`，走既有 catalog 文件管线

- 聚合被应用（成为当前 Codex 供应商）时，在 codex live 写入的**有效配置构建**处合成：
  `settings_config.modelCatalog = { models: [{ model, displayName? }, ...] }`（槽位序；
  `default_model` 命中则对应槽位置顶）。既有 `prepare_codex_config_text_with_model_catalog`
  管线把它落成 `~/.codex/cc-switch-model-catalog.json`（slug=model）+ `model_catalog_json`
  指针——**不新写文件格式**（#5937 同路，settings 级合成 + 既有落盘）。
- 顶层默认模型：seed TOML 的 `model = default_model ∪ slots[0].model`。
- 条目模板走既有三级来源（models_cache.json → CLI → 静态兜底），tool profile 统一取
  NativeResponses 形状（槽位目标的 tool profile 各异是事实约束，v1 取统一形状，
  显示与路由不受影响）。

### D3. 控制面：聚合的 live 写入合成 seed（不污染 settings_config）

- 聚合供应商按设计无端点无凭据，`settings_config` 为空对象。`sync_codex_live` 的
  校验（auth 必须对象、config 必须字符串）在聚合时**由有效配置合成满足**，与
  #5937 的 seed TOML + auth 占位同构（放 `build_effective_provider_for_live` 类入口，
  一处合成，切换与接管共用）：

```toml
model = "<default_model ∪ slots[0].model>"
model_provider = "cc-switch-aggregate"
disable_response_storage = true

[model_providers.cc-switch-aggregate]
name = "cc-switch Aggregate"
requires_openai_auth = true
base_url = "http://127.0.0.1:15721"   # 由 proxy 配置派生，不硬编码
wire_api = "responses"
```

  `auth = { OPENAI_API_KEY: "PROXY_MANAGED" }`（占位，代理注入真实凭据）。
- 聚合自身 `settings_config` 保持空（校验短路，同 Claude Desktop 聚合的
  「无端点无凭据」）；seed 只存在于写 live 的有效快照。

### D4. 数据面：打开 AppType 双门 + 改写点前移（一次改写覆盖三种上游形态）

- `handler_context.rs:160` 与 `forwarder.rs:1312` 的 `AppType::ClaudeDesktop` 门放宽为
  `ClaudeDesktop | Codex`（注释要求两层一致）；`resolve_target` 换 Codex 版查表函数
  `resolve_codex_target(db, "codex", aggregate, model)`（精确匹配 → 兜底）。
- **改写**：`aggregate_override` 既有改写点（`forwarder.rs:1316-1318`，body 顶层 `model`）
  对 Codex 同样生效——它在三条消费路径（原生透传 / Responses→Chat / Responses→Anthropic）
  **之前**执行，一次改写全覆盖（补上 #5937 指出的「原生透传无改写点」缺口，
  不动 transform 代码）。
- **防二次改写**：`apply_codex_upstream_model` 的 modelCatalog 白名单会在目标 catalog
  撞名时吞掉/误改改写结果（research §5.5）。规则：`aggregate_override.is_some()` 时
  **跳过** `apply_codex_upstream_model`（聚合层拥有模型名语义），三处调用点同规则。
- 「当前供应商」语义沿用 Claude 侧：命中后 `current_provider_id` 指向目标
  （防止成功回填把聚合切走）；`providers[0]` 同步替换 + 链截断（同 claude 路径的
  Task 4 修复）。`resolve` 对 Codex 的 `[1M]` 后缀做与 Claude 同款剥离后查表
  （forwarder 既有 mapped_body 剥离语义不变）。

### D5. 校验与删除保护：泛化既有实现

- 保存校验（`services/provider/mod.rs`）：slot 的 `model` 非空 + 全表唯一、
  `provider_id`/`upstream_model` 非空、目标存在性延到运行时（同 Claude 侧）、
  禁嵌套（聚合不得指向聚合）、已被引用者不得改造成聚合（反向检查）。
- 删除保护：`reject_if_referenced_by_aggregate` 泛化为同时查 `aggregateRoutes`
  与 `codexAggregateRoutes` 引用。
- 校验错误打 `AppError::Localized` 稳定 key（既有惯例，不锁文案）。

### D6. UI：扁平行编辑器（不复用四档卡）

- Codex 表单 `!isOfficial` 顶部聚合开关（同 Claude 形态）；开启 → 初始化
  `{slots: [], defaultTarget}`；关闭 → delete `meta.codexAggregateRoutes`。
- 槽位编辑 = 扁平行表格（模型名 | 目标供应商 Select | 上游模型 + 获取模型列表 |
  显示名）：**无档位行**（Claude 四档是 Desktop 特性）。上游模型复用
  `fetchModelsForConfig`（按供应商缓存，同 Claude 侧聚合）。
- defaultModel / 兜底目标：同 Claude 侧交互（下拉 + 「兜底目标」文案）。

## 3. 数据流

```
Codex CLI  →  config.toml model_provider=cc-switch-aggregate (base_url=本地代理)
           →  POST /v1/responses {model: "<slot.model>"}
           →  RequestContext：当前供应商=聚合 → resolve_codex_target（精确→兜底）
           →  aggregate_override：providers[0]=目标、body.model=upstream_model
           →  三种上游形态（原生透传 / Chat / Anthropic 转换）目标自身 adapter
```

控制面：聚合保存 → 校验（D5）；聚合应用 → 合成 seed + modelCatalog（D2/D3）→
catalog 文件落盘；删除/去聚合 → 既有 live 写入自然收敛（切到下一家重写 catalog）。

## 4. 测试计划

1. 数据模型 serde 往返 + `resolve_codex_target`（精确命中/兜底/悬空错误/唯一性）。
2. 有效配置合成：seed TOML 位置断言（base_url/wire_api 在表内、model 在顶层）、
   auth 占位、modelCatalog 槽位序 + default 置顶。
3. 改写：`aggregate_override` 存在时 `apply_codex_upstream_model` 跳过（白名单撞名
   不再吞改写）；原生透传路径 body.model 已是上游名（端到端 mock，参照
   `proxy/server.rs:620-740` 夹具 + `/v1/responses`）。
4. 校验/删除保护（同 Claude 侧测试形态，打 Localized key）。
5. 前端：类型 + 编辑器（提交形状断言）。

## 5. 已知限制（记录）

- catalog 条目统一 NativeResponses 形状；目标 tool profile 混杂时条目键集不逐目标适配。
- 无别名层、无 1M 变体行（Codex 是否消费 [1M] 变体未证实；查表已剥后缀故不坏）。
- 与上游 #5937 的 `aggregateRoutes` 仍互斥（Claude 侧既有碰撞不变）；本设计的
  `codexAggregateRoutes` 不新增碰撞面。
- Codex CLI 选择器读 catalog 文件为仓库外推断（research §2.1 证据边界），验收时确认。
