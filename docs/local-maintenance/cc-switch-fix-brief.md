# CC-Switch 配置持久化修复 —— 技术简报

日期：2026-09-20 ｜ 目标仓库：`farion1231/cc-switch` ｜ 影响版本：≤ 3.20.3

---

## 1. 现象

在 Claude Desktop「Developer → Configure third-party inference」面板中，凡是不属于"网关连接/模型路由"的设置，在以下任意时机后**全部回退为默认值**：

1. 在 CC-Switch 中**切换供应商**
2. **重启 Claude Desktop**（CC-Switch 会重申接管）
3. **重启 CC-Switch**

**实测时间线**（2026-09-17，Windows 11 / CC-Switch 3.20.3 / Claude Desktop 2.110.0.0 MSIX）：

```
13:20:17  CC-Switch 写入 profile（切到 Zhipu GLM）
13:21:28  CC-Switch 再次写入（切到 DeepSeek-OTN）
13:21:44  Claude Desktop 重启
→ profile 键数 15 → 7；autoModeEnabled / chatTabEnabled / toolSearchEnabled
  等用户设置全部回到 CC-Switch 模板值
```

面板右上角的 **"• CC Switch" 徽章**表示该配置已被外部托管（`configLibrary/_meta.json` 中条目名为 "CC Switch"）。

**影响规模**：从 Claude Desktop 自带的配置 schema 提取，面板可写、作用域含 `3p` 的设置为 **144 项**；CC-Switch 模板能保留的仅其中极少数。

---

## 2. 根因

**CC-Switch 的接管服务在每次写入时，用自己重建的 profile 对象整份覆盖目标文件。**

证据链：

1. **写入点**（当前 `main`）：
   `src-tauri/src/claude_desktop_config.rs:1007`
   ```rust
   write_json_file(&paths.profile_path, &profile)?;
   ```
   其中 `profile` 来自 `build_gateway_profile()` —— **只包含网关字段**（`inferenceGatewayBaseUrl` / `ApiKey` / `inferenceProvider` / `inferenceModels` 等）+ 少量硬编码的额外字段。

2. **CC-Switch 的数据模型里没有存放其余设置的位置**（这是它覆盖而非保留的深层原因）：
   全部 7 个 `claude-desktop` 供应商的 `meta` 键并集：
   ```json
   ["apiFormat", "claudeDesktopMode", "claudeDesktopModelRoutes", "usage_script"]
   ```
   → 非路由字段：**0 个**。

3. **文件是完整 JSON 文档，last-writer-wins**。平台（Anthropic）对"托管条目"没有合并契约；Claude Desktop 自己的写入是保留式的（只增改自己的字段），但 CC-Switch 的写入是替换式的 —— 两者相遇，保留式必然输。

**结论**：这是 CC-Switch 的结构性缺陷 —— 它对"它不拥有的字段"毫无概念，重建文档时必然丢弃。不是某个字段的疏漏；按字段补（如 #7449 补 `autoModeEnabled`）治不了标。

---

## 3. 解决方案：Read-Modify-Write 合并

**核心改动**（与已存在的 PR #5417 实现一致；该 PR 自 2026-07-15 起 MERGEABLE、CI 五个检查全绿，仅缺人工 review）：

```rust
// src-tauri/src/claude_desktop_config.rs  函数 apply_provider_to_paths_inner 内
// 替换原 1007 行：
//   write_json_file(&paths.profile_path, &profile)?;
let existing = read_json_or_empty(&paths.profile_path)?;   // 读磁盘现有文档
let merged = merge_profile(&existing, &profile);           // 合并
write_json_file(&paths.profile_path, &merged)?;

/// 合并策略：
/// 1. 从磁盘现有 profile 出发；
/// 2. 新 profile 携带的键全部覆盖（网关/路由字段始终刷新为新供应商的值）；
/// 3. 其余所有键原样保留（用户与 Desktop 的自定义字段存活）；
/// 4. 若新 profile 不含 inferenceModels，则删除旧值
///    —— 防止上一个供应商的模型映射泄漏到本次切换。
fn merge_profile(existing: &Value, new_profile: &Value) -> Value {
    let mut merged = existing.clone();
    let Some(merged_obj) = merged.as_object_mut() else { return new_profile.clone(); };
    let Some(new_obj) = new_profile.as_object() else { return new_profile.clone(); };
    for (key, value) in new_obj {
        merged_obj.insert(key.clone(), value.clone());
    }
    if !new_obj.contains_key("inferenceModels") {
        merged_obj.remove("inferenceModels");
    }
    merged
}
```

### 必须配套的测试

| 测试 | 断言 |
|---|---|
| `claude_desktop_apply_preserves_non_gateway_profile_fields` | 预写含 `chatTabEnabled`/`autoModeEnabled`/`inferenceCredentialKind`/`managedMcpServers` 的 profile → 应用供应商后网关字段刷新为新值、**非网关字段原样保留** |
| `claude_desktop_apply_clears_stale_inference_models_when_new_provider_has_none` | 新供应商无 `inferenceModels` 时，旧映射**不泄漏** |
| （建议补充）Desktop 重启重申场景 | 连续两次 `apply_provider_to_paths` 幂等、用户字段两次都存活 |

（前两个测试 PR #5417 已含，可直接参考其 diff。）

### 验证口径（改完后在本机）

1. 面板里设一个开关（如关掉 Auto mode）→ 切换供应商 → 面板值仍在；
2. 切回原供应商 → 路由跟随新供应商、开关值不变；
3. 重启 Claude Desktop → 开关值不变。

### 构建与部署（Rust/Tauri）

```bash
git clone https://github.com/farion1231/cc-switch
# 应用上述补丁
cargo test                 # 按仓库实际 crate 布局执行
cargo tauri build          # 产出安装包
```

**风险提示**：自建版会替换本机的官方签名二进制，而 CC-Switch 持有全部供应商密钥并代理全部流量 —— 仅本机自用，勿分发。

---

## 4. 当前状态与防线

- **上游**：PR #5417 OPEN / 未合并（2026-09-17 曾提交支持性留言与取证）；Issue #3329 已复活（含结构性证据）。
- **本地防线（正在运行）**：`claude3p-profile-guard` 守护进程（`D:\Workspace\Claude Desktop\Code\Tmp\claude3p-profile-guard\`），以"取差值"方式保护 144 项面板设置 —— 源码修复落定前兜底，合并后可降为可选。
- **合并成功后的预期**：CC-Switch 升级即根治；守护保留作兜底（它还能覆盖 Claude Desktop 客户端自身的写入与 schema 迁移场景）。
