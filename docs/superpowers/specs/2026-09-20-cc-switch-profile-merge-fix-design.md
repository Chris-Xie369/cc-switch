# 设计：CC Switch 3P profile 覆盖缺陷本地修复（合并语义）

- 日期：2026-09-20
- 状态：设计已获用户逐节确认（两节均确认）
- 上游：PR [farion1231/cc-switch#5417](https://github.com/farion1231/cc-switch/pull/5417)（OPEN / MERGEABLE，提交 `e71fe6998`）
- 分支：`fix/profile-merge`（基于 tag `v3.20.3`，仅本地，不推送）

---

## 1. 背景与目标

CC Switch 每次应用/切换 Claude Desktop 供应商时，用 `build_gateway_profile()` 重建的模板**整份覆盖** 3P profile 文件（`src-tauri/src/claude_desktop_config.rs` 中 `write_json_file(&paths.profile_path, &profile)`），对非网关字段（`chatTabEnabled`、`autoModeEnabled`、`managedMcpServers` 等 144 项面板可写设置）毫无概念，重建即丢弃。本机已有 claude3p-profile-guard 守护进程兜底，但根治需改上游源码。

**目标**：本机编译自用的 CC Switch 安装包，profile 写入改为**合并语义**（读现有 → 自有键覆盖 → 未知键保留），彻底消除覆盖事故。

## 2. 已确认的关键决策

| 决策 | 结论 |
|---|---|
| 产出形态 | 本地编译自用（完整安装包部署） |
| 部署方式 | 构建完整 NSIS 安装包，覆盖安装到现有目录 |
| 基线 | tag `v3.20.3`（与安装版完全一致）+ PR #5417 补丁 |
| 版本号 | `3.20.3-local`（`tauri.conf.json` 与 `src-tauri/Cargo.toml` 同步改，tauri-build 校验两者一致；`package.json` 不动） |
| 自动更新防覆盖 | 利用仓库既有 `createUpdaterArtifacts: true`：本地构建自动生成本地签名密钥对并嵌入本地公钥 → 官方更新签名校验必然失败 → 官方版无法覆盖本地构建。**不改任何配置** |

## 3. 补丁内容（与 PR #5417 完全一致）

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

**不改动的部分**：官方供应商恢复路径（`restore_official_at_paths_inner` 删除 profile）、rollback 快照机制、Claude Code / Codex / Gemini 路径、前端代码。

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
git cherry-pick e71fe6998                    # 冲突则手工应用
# 改版本号: src-tauri/tauri.conf.json + src-tauri/Cargo.toml → 3.20.3-local
cargo test                                   # 在 src-tauri/ 下；先测后装，见 §6
pnpm install
pnpm tauri build --bundles nsis              # 跳过 MSI（需 WiX），NSIS 自动下载
```

- 产物（预期路径，以实际产物为准）：`src-tauri/target/release/bundle/nsis/CC Switch_3.20.3-local_x64-setup.exe`
- 首次构建 15-40 分钟，磁盘约 4-6GB（target 目录）
- **构建后验证**：在产物 exe 中检索官方 pubkey 字符串（`dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEM4MDI4` 前缀）应**不存在** = 本地公钥已嵌入

## 6. 测试标准（打补丁后、pnpm install / tauri build 之前先跑）

```bash
cargo test   # 在 src-tauri/ 下；单 crate，非 workspace
```

通过标准：

- 新增两测试通过（字段保留 / 陈旧模型清理）
- 既有 `claude_desktop_apply_writes_3p_profile_and_meta` 不回归
- 全量测试套件在补丁分支上通过；若有个别失败，须在干净 `v3.20.3` 上**同样失败**（即预存失败、与本补丁无关）才可豁免

## 7. 部署（顺序执行，每步有回滚点）

1. **备份**：官方 `cc-switch.exe` → `cc-switch.exe.official-3.20.3`（同目录）；记录官方安装包 URL（GitHub release v3.20.3 资产）
2. **退出**：从托盘正常退出 cc-switch（**不直接 kill**——26MB SQLite DB，避免写坏；僵死才 kill 并靠 `.bak` 恢复）
3. **安装**：运行 NSIS 安装器覆盖安装到 `%LOCALAPPDATA%\Programs\CC Switch\`；用户数据在 `%USERPROFILE%\.cc-switch\`，不迁移不重建
4. **启动**：确认进程版本 = 3.20.3-local

## 8. 验收口径（逐条实测，不看日志就宣称修好）

1. 面板设开关（关 Auto mode）→ 切换供应商 → 值仍在
2. 切回原供应商 → 路由跟随新供应商、开关值不变
3. 重启 Claude Desktop → 值不变
4. 判据：profile 键数不再从 19 骤降到 7

## 9. 收尾与回滚

- 验收通过后**询问用户**是否卸载 guard（工作区文档定位为「合并后可降为可选/卸载」）
- 回滚：重装官方 3.20.3 安装包；备份 exe 作二次保障
- 退出策略：上游 PR #5417 合并发版后，升级官方版、放弃本地构建

## 10. 风险清单（已评估，可接受）

- 本地构建无代码签名 → SmartScreen 可能警告（点「仍要运行」）
- 「检查更新」显示官方新版本、安装时报签名错（已知表象，非故障）
- 首次构建耗时长 + 磁盘占用
- main 上 v3.20.3 之后的 14 个上游提交（Kimi/MiniMax 预设等）**不包含**在本构建中——用户场景（Windows 3P + Zhipu/DeepSeek）不受影响
- 自建版持有全部供应商密钥并代理全部流量——仅本机自用，勿分发
