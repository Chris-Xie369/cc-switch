# 独立代码审查报告：聚合账号与认证边界（M03/M04）

- 任务：task_9b93b0f7421ed396（高风险）
- 审查对象：D:/Sources/Code/agent-org/dist/agent-org/cc-switch-fixes-2026-10-04-1ebd221f/account-frozen（只读，仅限冻结目录；控制器确认冻结工件未变）
- 登记审查者：cc-switch-account-boundary-reviewer / project-llm / glm-4.7 / hermes-cli / high。模型/供应商/家族为宿主请求侧声明（review-model-classification.json 明确 provider_identity_attested=false），本报告不构成供应商身份认证；实际会话 ID 以宿主为准。
- 说明：本审查在同一会话内实际完成；前次交付因宿主 CLI exit_code=1 未被控制器导入，本版为同一已完成审查的重新交付，结论与发现未改动。
- 结论：**passed**（0 Critical / 0 Major / 4 Minor）

## 一、证据完整性核验

- manifest.json 列出的 14 个 source/diff/报告文件逐一用 sha256sum 复核，全部与清单一致（含 4 个变更源码、5 个上下文源码、4 个 diff、实施报告）。7 个 evidence 文件按内容直接阅读核验（未单独复算哈希）。
- `read_codex_auth_json_fallback` 在冻结生产源码中全文检索为 0 命中——auth.json 回落路径确已整体删除，不是仅改调用点。
- 实施报告的关键声明与证据吻合：RED 5 项失败（含 provider 兜底因无 AppHandle 报错的旧行为）、GREEN 96 passed/0 failed/2936 filtered、cargo check --offline --lib 通过（account-production-check.txt 显示 Checking→Finished）。报告将验证范围如实限定为『转发层测试』，未见夸大。
- admission-rejection.json 记录了 openai 家族审查者被同族冲突拒收、改用既有 glm-4.7/zai 路由的过程，与『不同声明家族、不作供应商认证』的表述一致；治理记录自洽。

## 二、独立代码追踪（不依赖报告结论）

### 1. 请求来源与模型改写解耦
handler_context.rs:162-166 单独计算 `is_aggregate_request`（ClaudeDesktop→is_aggregate_provider，Codex→is_codex_aggregate_provider，两套路由键互不相通），与 `aggregate_override` 独立；:305 经 `with_aggregate_route` 一次性传入 forwarder，是冻结树内唯一生产调用点。`resolve_codex_target`（aggregate.rs:1540-1585）：精确槽位命中→改写为 upstream_model；未命中→DefaultTarget::ProviderId 兜底**不改写模型**（upstream=None）；SlotId 兜底→保留该槽 upstream；悬空 SlotId/缺失目标供应商→显式报错。1M 后缀剥离、两侧 trim、无前缀误匹配均有对应单测。

### 2. 聚合官方目标认证注入与占位符处置
forwarder.rs:2102-2118 注入条件改为 `codex_official_auth_passthrough && is_aggregate_request && auth_headers.is_empty()`——供应商兜底（无模型改写）时官方卡 `extract_auth` 返回 None（codex.rs:1019 起，extract_key 对空 auth 对象为 None），auth_headers 为空，注入照常发生：这正是『改不改模型不决定认证来源』的核心修复，并由环回端到端测试三断言（Authorization=SYNTHETIC-SELECTED-TOKEN、chatgpt-account-id=selected-workspace、model 按三种 default_target 分别为透传/槽位改写）实证。:1344 客户端认证校验仅在 `!is_aggregate_request` 时执行（客户端占位符不再误伤聚合路径）；:2354 客户端 Authorization 透传仅在 `!is_aggregate_request` 时保留，聚合请求必然以 Manager 现取令牌替换占位符。三处条件同步由 `aggregate_override.is_some()/is_none()` 迁移，非聚合路径逐字节不变（diff 核对）。

### 3. 账号解析 fail-closed（forwarder.rs:159-188）
选择来源唯一：`managed_account_id_for("codex_oauth")`（provider.rs:622-636，authBinding ManagedAccount 优先，兼容旧 githubAccountId 仅限 copilot）→ 无绑定则 `default_account_id()`（codex_oauth_auth.rs:1898-1909，仅取 Manager 内账号）→ 均无则报错。随后 `get_valid_token_for_account`（要求 id_token 可证明身份 + workspace 存在，缺一即 ParseError；缓存过期走账号级刷新锁，采纳磁盘 refresh 前强制 workspace 一致性校验）与 `chatgpt_account_id_for_account` 任一失败即统一报错。最后在组装头部前再拒绝空/空白令牌、PROXY_MANAGED 占位令牌、空 workspace。**不存在任何读取另一份登录文件改变账号的路径**；且测试通过在 CC_SWITCH_TEST_HOME 预置 SYNTHETIC-UNRELATED-TOKEN 的 auth.json 作为『毒饵』——若代码回归到文件回落，等值断言/err 断言必炸，tripwire 设计有效。账号语义与非聚合 CodexOAuth 路径（forwarder.rs:1988-1996 同一 binding→default 序列）一致。

### 4. 错误处理与凭据保密
所有失败收敛为同一提示『ChatGPT 未登录或所选账号不可用，请在 CC Switch 的 OpenAI 官方供应商重新登录』，不回显底层错误。注入测试（RefreshTokenInvalid + 含 SYNTHETIC-SECRET 的 NetworkError，覆盖显式绑定与默认账号 4 种组合）直接断言错误消息不含秘密字符串——这是防回显的回归护栏。代价是诊断信息同时丢失（见 MIN-01）。

### 5. 普通官方 / API-key 兼容
非聚合官方路径：客户端校验、占位符拒绝、Authorization 逐字节透传均保留（:1344、:2354 的 `!is_aggregate_request` 分支与改动前等价；既有测试 official_target_without_aggregate_keeps_client_passthrough_auth、managed 系列均绿）。非官方聚合目标不进注入分支，API-key 卡照常用自身凭据（aggregate_non_official_target_auth_path_is_untouched + 新增 normal_official_and_api_key_requests_keep_their_authentication，用错误 manager 验证不被注入）。

### 6. cfg(test) 接缝与私有数据隔离
生产影响面核对：`test_codex_oauth_manager` 字段与分支、`test_token_failure` 字段与注入检查、`fail_next_token_resolution_for_test`、测试模块 `#[path]` 挂载全部在 #[cfg(test)] 下；生产路径仍是 AppHandle→CodexOAuthState，无生产开关。测试全用合成令牌、tempfile 隔离 Manager 存储、Database::memory()、环回 127.0.0.1 接收端（LoopbackAdapter 仅改目的地，认证与转发用真实实现），CC_SWITCH_TEST_HOME 在 Drop 中恢复原值；新增/既有触碰环境变量的测试补 `#[serial_test::serial]`。未发现真实凭据、真实上游调用或 live 配置写入。未运行 OAuth 测试与完整应用（按审查约束），无法据此推断原生 UI 或真实上游验收——报告亦未做此声明，边界诚实。

## 三、发现的问题（均为 Minor）

1. MIN-01 失败路径无日志、底层原因完全丢弃（forwarder.rs:179/183/185），保密达标但排障性下降。
2. MIN-02 RED 证据行号与冻结测试文件不一致，RED 捕获自中间修订版测试文件（evidence/account-behavior-red.txt:16）；报告声称保留的导入错误记录不在冻结证据内。
3. MIN-03 证据仅覆盖转发层 96 测试 + cargo check --lib；全套 ~3000 测试未复跑，跨模块 CC_SWITCH_TEST_HOME/serial 交互未验证（报告已如实限定范围）。
4. MIN-04 `with_aggregate_override` 仍 pub 并保留旧语义推导，冻结树内无其他生产调用点、对旧调用方行为等价，但属易误用接口（forwarder.rs:387）。

未发现：Critical/Major 缺陷、账号静默替换路径、占位符外泄路径、生产 cfg 泄漏、报告与代码的实质性不符、相对基线（含 pre_existing_forwarder_dirty=true 的既有用户改动）的意外回归。

## 四、局限

- 仅审查冻结子集（9 个源码文件 + diff + 证据）；整个 crate 的其余调用点（如 lib.rs、server.rs、前端）不在冻结范围，无法核验。
- 未执行任何测试/构建/OAuth 流程，未做真实上游与 Tauri UI 验收；GREEN 证据为实施方留档，行号层面对照见 MIN-02。通过的单元测试不证明原生 UI 或真实上游验收。
- 7 个 evidence 文件按内容核验，未逐个复算哈希。
- 模型家族为宿主侧声明，本审查不构成供应商身份认证。

## 五、回退评估

四个 diff 相对备份基线可逆应用；报告的回退方案（恢复受影响源码 + 原有用户 OAuth 版本、保留修复后文件与证据、禁用整仓 reset）与本目录留档一致，可行。

## 六、结论

本阶段（M03/M04）实现与声明相符：显式选择/默认账号失效一律显式失败，不再回落无关登录文件；聚合认证来源由请求来源决定而非可选模型改写；官方兜底、槽位兜底、精确路由行为正确；普通官方/API-key 路径保持不变；测试隔离与 cfg(test) 接缝干净。允许 passed，4 项 Minor 建议随后续批次处理（日志保真、RED 证据规范、全量测试留档、收紧旧构建器）。

控制者运行核对：原审查进程 exit1 保留；同一会话最终收尾 exit0，完整审查输出已获得。模型/effort为宿主请求参数，非供应商身份认证。控制者逐项校验manifest中20个文件及10个当前源码/报告，均匹配。
