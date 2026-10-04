# 聚合路由 OAuth 注入实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: subagent-driven-development 或 executing-plans。

**Goal:** Codex 聚合路由到 OpenAI Official 时，代理注入 OAuth Manager 的令牌替代客户端的 PROXY_MANAGED 占位符。

**Spec:** `docs/superpowers/specs/2026-10-03-aggregate-oauth-target-design.md`（D1–D5）

## Global Constraints

- 只动 `src-tauri/src/proxy/forwarder.rs`
- 非 official 目标的认证路径**逐字节不变**
- `aggregate_override.is_none()` 时的官方校验路径**逐字节不变**
- Rust 测试 `cargo test --release`；fmt 0

---

### Task 1: OAuth 注入（单任务，含测试）

**Files:**
- Modify: `src-tauri/src/proxy/forwarder.rs`
- Test: 同文件 `mod tests`

**核心改动（两处）：**

**改动 A**（~L1297）：官方校验守卫——当 `self.aggregate_override.is_some()` 时跳过 `validate_codex_official_authorization`。该函数检查的是**客户端**的 Authorization 头（聚合场景下 = PROXY_MANAGED 占位符，必然触发报错）；聚合路径的凭据由代理注入（改动 B），不依赖客户端携带官方凭据。

```rust
// 现有代码（L1297 附近）：
validate_codex_official_authorization(headers, provider, ...)?;

// 改为：
if self.aggregate_override.is_none() {
    validate_codex_official_authorization(headers, provider, ...)?;
}
```

**改动 B**（上游请求头组装处）：当 `aggregate_override.is_some()` 且目标为官方供应商时，用 `CodexOAuthManager` 的 access_token 替换出站 Authorization 头。定位 `send_request` / `build_auth_headers` 中 official 分支的令牌来源——正常路径透传客户端的 OAuth 头，聚合路径改为从 Manager 取。

注入条件（全部满足才生效）：
- `self.aggregate_override.is_some()`
- `app_type == AppType::Codex`
- 目标供应商 `is_codex_official_provider(provider)` 或等效判定

错误处理：Manager 无有效令牌 → `ProxyError::AuthError("ChatGPT 未登录或令牌已过期，请在 CC Switch 切到 OpenAI Official 供应商重新登录")`。

**测试（TDD）：**
1. 单测：aggregate_override + official target → 出站 Authorization 含 Manager 令牌（非 PROXY_MANAGED）
2. 单测：aggregate_override + 非 official target → 认证路径不变（不受改动影响）
3. 单测：aggregate_override.is_none() + official → 校验路径不变（不受改动影响）
4. 单测：Manager 无令牌 → 明确报错
5. e2e：mock OAuth Manager + axum mock OpenAI 上游 → 断言出站 Authorization == Bearer <mock-token>
