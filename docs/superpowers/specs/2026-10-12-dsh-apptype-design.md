# DeepSeek Harness AppType（R2）设计

日期：2026-10-12 ｜ 状态：待用户确认 ｜ 前置：R1 已交付（手工配置 + x-api-key 网关补丁 078b0d9c）

## 目标

cc-switch 新增 **DeepSeek Harness** 应用（AppType `deepseek-harness`，additive 模式）：供应商卡读写 `~/.dsh/settings.yaml` 的 `llm-pi-ai.providers` 与 `~/.dsh/.credentials.yaml` 的 `refs:`；聚合槽位自动同步（R1 的手工工作自动化）。上游化候选（Pi 先例）。

## 侦察结论（代码实证）

| 事实 | 出处 |
|---|---|
| DSH = pi-ai 下游：`llm-pi-ai.providers.<route>`（api/baseURL/apiKeyEnv/displayName/models[]），凭据经 apiKeyEnv → env → `.credentials.yaml refs:` → `.env` 解析 | asar 内嵌 `@deepseek-ai/dsh-llm-pi-ai` README |
| 家目录：cc-switch 设置覆盖 → `DSH_HOME` env → `~/.dsh`（与 harness 自身一致） | asar `DSH_HOME` 常量 + pi_override_dir 先例 |
| additive 模式先例：OpenCode/OpenClaw/Hermes/Pi/Mcode——「供应商直接管进配置文件，无切换无 live sync」 | `app_config.rs:441 is_additive_mode` |
| 聚合同步挂点：`claude_desktop_config.rs:143 apply_provider` 已有 `sync_cli_model_picker(db, routes)` warn-only 自愈模式，DSH 同步并排挂同点 | 现有代码 |
| YAML：仓库已有 `serde_yaml 0.9`（hermes 在用） | Cargo.toml:70 |
| Pi 表单 2161 行——DSH 表单远小于此（5 字段） | PiProviderForm.tsx |

## 架构

```
后端                                     前端
app_config.rs   AppType::DeepSeekHarness  types.ts AppId "deepseek-harness"
dsh_config.rs   (新) YAML 适配器          appConfig.tsx 注册（图标/名称/可见性）
  read_providers / upsert / remove       DshProviderForm（5 字段结构化表单）
  upsert_credential_ref / remove_ref     DshPage 聚合同步按钮
  sync_dsh_aggregate(db, routes)         i18n en/zh
commands/dsh.rs  CRUD + sync 命令
services/provider  additive CRUD 接线
```

## 关键设计

1. **归属（key-field 精神）**：cc-switch 只拥有 `llm-pi-ai.providers.<route>` 各键与 `refs:<envName>`；文件其余内容（harness Models 页写的一切、`records:` OAuth 区）原样保留。serde_yaml round-trip 会丢注释——**接受**（该文件本就由 harness 编辑器整写，非注释承载）。
2. **聚合同步**：`sync_dsh_aggregate(db, routes)` 在两处触发——①`apply_provider` 里 model_picker 同步旁（warn-only 自愈）；②DSH 页「立即同步」按钮。产物=R1 手工内容（route `cc-switch`、anthropic-messages、`/claude-desktop` 前缀、11 槽、`CC_SWITCH_GATEWAY_TOKEN` ref+token）。
3. **供应商卡 CRUD**（决策点 B 才有）：字段 displayName / api（三选一）/ baseURL / apiKey / models（id+name 列表）。apiKey 存 DB 卡（与其他 app 明文一致），落盘时写 `refs: CC_SWITCH_DSH_<ROUTE 大写>`。删除卡 = 删 provider 键 + 删 ref（仅当 ref 值匹配才删，防误删手写）。
4. **DB**：app_type TEXT 直接存 `deepseek-harness`，**无 schema 迁移**（v20 不变）。
5. **凭据冲突**：聚合 route 固定名 `cc-switch`、env 名 `CC_SWITCH_GATEWAY_TOKEN`；手动卡 env 名前缀 `CC_SWITCH_DSH_`，无碰撞。

## 决策点（请用户拍板）

- **D1 v1 范围**：
  - **A** 仅聚合同步（自动+按钮）——无手动供应商 CRUD（其他 provider 在 harness 自己的 UI 加）。工作量 ~1/3。
  - **B** 聚合同步 + 手动供应商卡 CRUD（推荐——"AppType 完整支持"的本义，上游 PR 需要它）。
- **D2 注释取舍**：接受 serde_yaml 丢注释（推荐）vs 文本级手术保留（脆弱，不推荐）。
- **D3 i18n 范围**：v1 只做 en/zh（推荐，缺失 locale 回落 en）vs 全 locale。

## 上游化路径

独立 PR（不动 #7785 三连）：「feat: DeepSeek Harness app support」。卖点：pi-ai 生态第二成员（Pi 先例）+ x-api-key 网关补丁（078b0d9c 可并入或另提）。

## 验收场景（GWT）

- Given MoA 聚合生效 When 在 CC Switch 切换 Claude Desktop 供应商 Then `~/.dsh/settings.yaml` 的 cc-switch route 槽位与 DB 一致、token ref 最新
- Given DSH 页有同步按钮 When 点击 Then 立即重写 cc-switch route（即使 Desktop 无切换）
- Given（B）用户建 DSH 卡 name=foo api=openai-completions baseURL=X key=K models=[m1] When 保存 Then settings.yaml 出现 providers.foo + refs: CC_SWITCH_DSH_FOO=K；harness Models 页可见可用
- Given（B）删除卡 foo When 执行 Then provider 键与匹配的 ref 消失，其他内容原样
- Given 用户在 harness Models 页改了 settings.yaml When CC Switch 下次同步/CRUD Then 仅动自有键，harness 的改动保留
