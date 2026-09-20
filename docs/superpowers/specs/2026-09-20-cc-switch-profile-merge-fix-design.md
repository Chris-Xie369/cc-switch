# 设计：CC Switch 3P profile 覆盖缺陷本地修复（合并语义 + 显示名可配置）

- 日期：2026-09-20
- 状态：设计已获用户逐节确认；用户补充要求「删除 guard」+「左下角显示名可配置且持久化」后修订本版
- 上游：PR [farion1231/cc-switch#5417](https://github.com/farion1231/cc-switch/pull/5417)（OPEN / MERGEABLE，提交 `e71fe6998`）
- 分支：`fix/profile-merge`（基于 tag `v3.20.3`，仅本地，不推送）

---

## 1. 背景与目标

CC Switch 每次应用/切换 Claude Desktop 供应商时，用 `build_gateway_profile()` 重建的模板**整份覆盖** 3P profile 文件（`src-tauri/src/claude_desktop_config.rs` 中 `write_json_file(&paths.profile_path, &profile)`），对非网关字段（`chatTabEnabled`、`autoModeEnabled`、`managedMcpServers`、`deploymentDisplayName` 等）毫无概念，重建即丢弃。本机 claude3p-profile-guard 守护进程兜底，但其自启静默失效曾导致 9/20 事故，且属补丁式方案。

**目标**（本机编译自用）：

1. profile 写入改为**合并语义**（读现有 → 自有键覆盖 → 未知键保留），消除覆盖事故；
2. 左下角显示名（`deploymentDisplayName` / `deploymentDisplaySubtitle` / `endUserAttribution` 三键）纳入 CC Switch 自有字段，**在 CC Switch 设置里可配置**，跨切换/重启持久；
3. **删除 guard**（停进程、清自启项，脚本目录备份后清理）。

身份行/问候语的 "Jason"（`os.userInfo().username` 兜底）不在范围内——report §2.1 已证实无配置键可改，用户已确认接受。

## 2. 已确认的关键决策

| 决策 | 结论 |
|---|---|
| 产出形态 | 本地编译自用（完整安装包部署） |
| 部署方式 | 构建完整 NSIS 安装包，覆盖安装到现有目录 |
| 基线 | tag `v3.20.3`（与安装版完全一致）+ 两个补丁（见 §3） |
| 版本号 | `3.20.3-local`（`tauri.conf.json` 与 `src-tauri/Cargo.toml` 同步改，tauri-build 校验两者一致；`package.json` 不动） |
| 自动更新防覆盖 | **本地构建禁用 updater**：移除 `plugins.updater` 的 `pubkey`/`endpoints` 与 `createUpdaterArtifacts`。`lib.rs` 既有逻辑在配置不完整时优雅跳过 updater 插件（注释原文：「若配置不完整（如缺少 pubkey），跳过 Updater 而不中断应用」）→ 应用不检查更新、无假升级提示、官方版无法覆盖本地构建，且构建无需签名密钥。**为何不是「靠签名天然防覆盖」**：Tauri 的 pubkey 来自 `tauri.conf.json`，构建时不会自动替换（已核实官方 exe 中内嵌该 pubkey）；而 `3.20.3-local` 在 semver 中**低于**正式版 `3.20.3`（预发布 < 正式），保留 updater 反而会主动提示升级到官方版，一点即覆盖修复 |
| 构建次数 | **一次构建**包含两个补丁（补丁小、回归面窄；验收分两步执行，见 §8） |
| guard 处置 | 验收第二步前卸载：备份目录 → 停进程 + 清 HKCU Run 自启项（若 `--uninstall` 会删脚本文件，则改为手动清 Run 键 + kill，保留脚本目录作回退） |

## 3. 补丁内容

### 3.1 补丁 A：合并语义（与 PR #5417 完全一致）

单文件 `src-tauri/src/claude_desktop_config.rs`，+101/-1：

1. `apply_provider_to_paths_inner` 内，将 `write_json_file(&paths.profile_path, &profile)?;` 替换为：
   ```rust
   let existing = read_json_or_empty(&paths.profile_path)?;
   let merged = merge_profile(&existing, &profile);
   write_json_file(&paths.profile_path, &merged)?;
   ```
2. 新增 `merge_profile(existing: &Value, new_profile: &Value) -> Value`：
   - 从磁盘现有 profile 出发，新 profile 携带的键全部覆盖，其余键原样保留；
   - 新 profile 不含 `inferenceModels` 时删除旧值（防止上一供应商模型映射泄漏）。
3. 新增两个测试：
   - `claude_desktop_apply_preserves_non_gateway_profile_fields`
   - `claude_desktop_apply_clears_stale_inference_models_when_new_provider_has_none`

**应用方式**：`git cherry-pick e71fe6998`；若与 v3.20.3 冲突，按 PR diff 手工应用（本机 `D:\Workspace\Claude Desktop\Code\Tmp\ccswitch_claude_desktop_config.rs` 是 3.20.3 版源码副本，可对照）。

### 3.2 补丁 B：显示名三键纳入 CC Switch 自有字段（新增功能）

**数据模型**（`src-tauri/src/settings.rs`）：

```rust
pub struct ClaudeDesktopDisplaySettings {
    pub name: String,        // deploymentDisplayName
    pub subtitle: String,    // deploymentDisplaySubtitle
    pub attribution: bool,   // endUserAttribution
}
// AppSettings 新增字段（serde default = None，功能默认关闭、行为不变）：
pub claude_desktop_display: Option<ClaudeDesktopDisplaySettings>,
```

**写入路径**：新增独立函数 `inject_display_settings(profile: &mut Value, display: Option<&ClaudeDesktopDisplaySettings>)`，在 `apply_provider_to_paths_inner` 的 `match` 之后、merge 之前调用（**不改** `build_gateway_profile` 的签名）。`display` 为 `Some` 且 `name` 非空时把三键注入 profile；`None` 或 `name` 为空时**不写**（磁盘旧值由合并保留）。`apply_provider_to_paths_inner` 通过 `crate::settings::get_settings()` 取值（既有内存缓存模式，与其他 config 写入器一致）。

**前端 UI**（React + TS，遵循既有 SettingsPage 模式）：

- 新组件 `src/components/settings/ClaudeDesktopDisplaySettings.tsx`：一个「管理 Claude Desktop 显示设置」开关（对应 Option 门控）+ 两个文本输入（显示名/副标题）+ 一个开关（endUserAttribution，默认 false，与用户当前磁盘值一致）；
- 副标题输入为空时按空串写入（用户可有意清空）；**显示名为空 = 未配置**（后端不写三键，避免启用开关却不输名字时把显示名抹空）；
- 注册进 `SettingsPage.tsx` 既有分栏；
- i18n 补 zh/en 键（`src/i18n/` 目录）；
- 设置走既有 `useSettings` hook + `settingsApi`（AppSettings 已整体序列化到前端，无需新命令）。

**测试**（Rust 单测，`claude_desktop_config.rs` tests mod，直接测 `inject_display_settings`）：

- `inject_display_settings_writes_keys_when_configured`：display 为 Some 且 name 非空 → profile 含三键且值正确；
- `inject_display_settings_omits_keys_when_none`：display 为 None → profile 不含三键（磁盘旧值由合并保留）；
- `inject_display_settings_omits_keys_when_name_empty`：display 为 Some 但 name 为空 → 不写三键。

**为何不写成经 `apply_provider_to_paths` 的集成测试**：该路径读全局 settings store，测试若要走集成就得调 `update_settings`，而它会写用户**真实的**设置文件，且全局可变状态会在并行测试间互相干扰。故单测覆盖注入逻辑；**接线正确性由 §8 验收第二步覆盖**（在 UI 改显示名 → 切换供应商 → 观察 profile 实际变化）。

**默认行为保证**：功能默认关闭（None）→ 安装后不改变任何现有行为；磁盘上已恢复的 Chris/Gateway/false 三键由补丁 A 的合并永久保留。

### 3.3 补丁 C：禁用本地构建的 updater（配置改动）

`src-tauri/tauri.conf.json`：移除 `plugins.updater` 的 `pubkey` 与 `endpoints`、移除 `bundle.createUpdaterArtifacts`。不改任何 Rust 代码——`lib.rs` 既有的 `if let Err(e) = app.handle().plugin(...)` 分支会在配置不完整时记录 warning 并跳过 updater 插件，应用正常运行。

### 3.4 不改动的部分

官方供应商恢复路径（`restore_official_at_paths_inner` 删除 profile）、rollback 快照机制、Claude Code / Codex / Gemini 路径、其余前端与后端代码。

## 4. 工具链（一次性安装，总下载约 2-4GB）

| 组件 | 版本 | 安装方式 |
|---|---|---|
| Rust | 1.95（`rust-toolchain.toml` 钉死，rustup 自动遵循） | winget `Rustlang.Rustup` |
| MSVC 链接器 | VS Build Tools 2022（C++ 工作负载 + Windows SDK） | winget `Microsoft.VisualStudio.2022.BuildTools` |
| pnpm | 10.12.3（仓库 `packageManager` 字段） | corepack enable 或 npm i -g pnpm@10 |
| Node | 已装 v24.15.0（无 engines 限制） | 已具备 |
| NSIS | tauri CLI 构建时自动下载 | 自动 |

## 5. 构建流水线

```bash
git checkout -b fix/profile-merge v3.20.3   # 已完成
git cherry-pick e71fe6998                    # 补丁 A；冲突则手工应用
# 补丁 B（settings.rs / claude_desktop_config.rs / 前端组件 / i18n）
# 补丁 C: tauri.conf.json 移除 plugins.updater 的 pubkey/endpoints + createUpdaterArtifacts
# 改版本号: src-tauri/tauri.conf.json + src-tauri/Cargo.toml → 3.20.3-local
pnpm install
cargo test                                   # 在 src-tauri/ 下；先测后构建，见 §6
pnpm tauri build --bundles nsis              # 跳过 MSI（需 WiX），NSIS 自动下载；无需签名密钥
```

- 产物（预期路径，以实际产物为准）：`src-tauri/target/release/bundle/nsis/CC Switch_3.20.3-local_x64-setup.exe`
- 首次构建 15-40 分钟，磁盘约 4-6GB（target 目录）
- **构建后验证**：产物 exe 中**不应**再含官方 pubkey 字符串（`dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEM4MDI4`）——确认补丁 C 生效、updater 已从本地构建中移除

## 6. 测试标准（打补丁后、pnpm install / tauri build 之前先跑）

```bash
cargo test   # 在 src-tauri/ 下；单 crate，非 workspace
```

通过标准：

- 补丁 A 两测试 + 补丁 B 两测试全部通过
- 既有 `claude_desktop_apply_writes_3p_profile_and_meta` 不回归
- 全量测试套件在补丁分支上通过；若有个别失败，须在干净 `v3.20.3` 上**同样失败**（即预存失败、与本补丁无关）才可豁免

## 7. 部署（顺序执行，每步有回滚点）

1. **备份**：官方 `cc-switch.exe` → `cc-switch.exe.official-3.20.3`（同目录）；记录官方安装包 URL（GitHub release v3.20.3 资产）
2. **退出**：从托盘正常退出 cc-switch（**不直接 kill**——26MB SQLite DB，避免写坏；僵死才 kill 并靠 `.bak` 恢复）
3. **安装**：运行 NSIS 安装器覆盖安装到 `%LOCALAPPDATA%\Programs\CC Switch\`；用户数据在 `%USERPROFILE%\.cc-switch\`，不迁移不重建
4. **启动**：确认进程版本 = 3.20.3-local

## 8. 验收口径（逐条实测，不看日志就宣称修好）

**第一步：持久化**（guard 仍在运行，与磁盘值一致故无冲突）：

1. 面板设开关（关 Auto mode）→ 切换供应商 → 值仍在
2. 切回原供应商 → 路由跟随新供应商、开关值不变
3. 重启 Claude Desktop → 值不变；左下角仍为 Chris / Gateway（三键由合并保留）
4. 判据：profile 键数不再从 19 骤降到 7

**第二步：卸载 guard 后验证可配置性**（guard 的 overlay 会恢复它记录的值，会与新配置值打架，**必须先卸载再测此步**）：

1. 备份 guard 目录 → 停进程 + 清 HKCU Run 自启项（保留脚本目录作回退）
2. CC Switch 设置 → 启用「管理 Claude Desktop 显示设置」→ 输入一个**与当前不同的**显示名（如 `Chris-Test`）→ 保存
3. 切换供应商 → `Chris-Test` 生效且其余字段不变
4. 重启 Claude Desktop → `Chris-Test` 仍在
5. 改回 `Chris` → 再次切换 → 跟随新值（证明可配置性，非一次性）

## 9. 收尾与回滚

- guard：卸载后不再守护；若日后需要，脚本目录仍在（备份 + 原目录双份），`--install` 可恢复
- 回滚：重装官方 3.20.3 安装包；备份 exe 作二次保障
- 退出策略：上游 PR #5417 合并发版后，升级官方版、放弃本地构建

## 10. 风险清单（已评估，可接受）

- 本地构建无代码签名 → SmartScreen 可能警告（点「仍要运行」）
- 「检查更新」会失败报错（updater 已被补丁 C 移除），属预期；本地构建的升级方式是按本设计重新构建或重装官方版
- 首次构建耗时长 + 磁盘占用
- main 上 v3.20.3 之后的 14 个上游提交（Kimi/MiniMax 预设等）**不包含**在本构建中——用户场景（Windows 3P + Zhipu/DeepSeek）不受影响
- 自建版持有全部供应商密钥并代理全部流量——仅本机自用，勿分发
- 删除 guard 后失去「Claude Desktop 客户端自身写入/schema 迁移」的兜底——合并语义保证 CC Switch 侧不再覆盖；Desktop 官方行为由官方负责
