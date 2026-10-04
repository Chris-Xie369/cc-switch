# 聚合路由支持 OAuth 型目标（GPT 模型入聚合）设计

日期：2026-10-03 · 状态：待用户审批

## 1. 问题

Codex 聚合路由到 OAuth 型供应商（OpenAI Official / ChatGPT）时代理报
「认证失败：已切换到官方供应商」——客户端用 PROXY_MANAGED 占位符连接代理，
代理将请求转发到目标供应商时无法注入 ChatGPT OAuth 令牌。

实测（2026-10-03）：聚合加槽位 `gpt → OpenAI Official → gpt-6.1-sol`，
经代理 /v1/responses 请求报上述错误。API-key 型目标（DeepSeek / Zhipu / OC）全通。

## 2. 方案

在 forwarder 的认证注入分支，当 `aggregate_override` 存在且目标供应商为
官方（`is_codex_official_provider`）时，从 `CodexOAuthManager` 取 OAuth
令牌注入 Authorization 头（替代 PROXY_MANAGED 占位符）。

### D1. 注入点

`RequestForwarder.forward()` 内、凭据组装处（现有 `AuthStrategy` 分发前后）。
当满足以下全部条件时注入：
- `self.aggregate_override.is_some()`
- `provider.uses_codex_official_auth()` 或 `is_codex_official_provider(provider)`
- `app_type == AppType::Codex`

### D2. 令牌来源

`CodexOAuthManager`（已有，管理 ChatGPT OAuth 令牌的获取与刷新）。
调用 `get_access_token()` 获取当前有效令牌；如需刷新由 Manager 自行处理。

### D3. 目标 base_url

官方供应商的 base_url 在代理内部路由到 ChatGPT backend-api（已有逻辑）。
聚合路径不改变 base_url 的解析方式——`resolve_codex_target` 返回的目标
provider 自带正确的 settings_config。

### D4. 错误处理

OAuth Manager 无有效令牌时返回明确错误（「ChatGPT 未登录或令牌已过期，
请切到 OpenAI Official 供应商重新登录」），不静默失败。

### D5. 不改的部分

- 聚合数据模型 / 校验 / UI（不变）
- API-key 型目标的认证路径（不变）
- Claude Desktop 聚合路由（不涉及——Anthropic 协议无 OAuth 官方目标）

## 3. 测试计划

1. 单测：aggregate_override + official target → OAuth Manager 被调用、令牌入头
2. 单测：aggregate_override + 非 official target → 不走 OAuth 路径
3. 单测：OAuth Manager 无令牌 → 明确错误
4. e2e：mock OAuth Manager + mock OpenAI 上游 → 断言出站 Authorization == Bearer <mock-token>

## 4. 预期收益

用户在 Codex MoA 聚合里加一个 `gpt → OpenAI Official → gpt-6.1-sol` 槽位，
在 codex `/model` 里选 `gpt` 即可用 ChatGPT 订阅额度——不再需要在 CC Switch
里切来切去。
