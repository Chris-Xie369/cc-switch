# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## 这个目录是什么

它是「CC Switch 全量覆盖 Claude Desktop 3P 配置」这一缺陷的**修复工作区**：收录分析与方案文档，并在 `src/` 下维护一份 cc-switch 的本地 clone——**本地构建自用，同时跟踪上游**。

> **维护与同步上游的完整流程见 [UPSTREAM-SYNC.md](UPSTREAM-SYNC.md)**（补丁清单、merge 流程、冲突热点、构建与部署命令）。本节以下内容为背景与现状。

真正会动的资产分布在别处：

| 资产 | 位置 | 状态 |
|---|---|---|
| CC Switch 源码（本地 clone） | `src/`（本目录下） | 分支 `fix/profile-merge`，基线 tag `v3.20.4`；`upstream`=官方、`origin`=自己的 fork（[Chris-Xie369/cc-switch](https://github.com/Chris-Xie369/cc-switch)），**绝不推送到 upstream** |
| 上游同步 / 本地维护流程 | [UPSTREAM-SYNC.md](UPSTREAM-SYNC.md) | 补丁清单、merge 流程、冲突热点、构建与部署命令、推送通道的坑 |
| 文档快照（异地备份） | `src/docs/local-maintenance/` | 本目录各文档的副本，随代码进了 fork。**改原件后需刷新快照**（命令见该目录 README） |
| 本地部署脚本 | [tools/install-local.bat](tools/install-local.bat) | 覆盖安装到现有目录（必须经它，Git Bash 会转写参数） |
| 修复设计 / 实施计划 | `src/docs/superpowers/specs|plans/` | 设计已获确认；计划含 8 个任务 |
| **聚合供应商**（Claude Desktop 多供应商共存） | spec `2026-09-22-claude-desktop-multi-provider-design.md`、plan 同名 | 目的：一次启用多家 → 重启一次 → 在 Claude 模型列表里自由选用任意一家的模型。实现完成（补丁 E），待本机验收 |
| 执行进度账本 | `src/.superpowers/sdd/progress.md` | 各任务状态、审查结论、环境坑记录 |
| 本地构建产物 | `src/src-tauri/target/release/bundle/nsis/CC Switch_3.20.4-local_x64-setup.exe` | 已构建 |
| 本机已安装版本 | `C:\Users\Jason\AppData\Local\Programs\CC Switch\` | **运行 3.20.4-local，验收通过**；官方 exe 备份为同目录 `cc-switch.exe.official-3.20.3` |
| guard 守护进程 | `D:\Workspace\Claude Desktop\Code\Tmp\claude3p-profile-guard\` | 独立 git 仓库，**已卸载**（2026-09-20 20:04；overlay 存于同目录 `overlay-preserved-20260920.json`，需要时 `--install` 可恢复） |
| Claude Desktop 3P profile | `%LOCALAPPDATA%\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json` | 22 键（无 guard 托管，由 CC Switch 3.20.4-local 的合并语义保护） |

动手前先读 [doc/claude-desktop-3p-profile-overwrite-report.md](doc/claude-desktop-3p-profile-overwrite-report.md)（结论与证据）和 [doc/cc-switch-fix-brief.md](doc/cc-switch-fix-brief.md)（补丁与验证步骤），再看 `src/.superpowers/sdd/progress.md` 的当前进度。本文件只做导航，不替代它们。

---

## 问题与三方关系

**根因一句话**：CC Switch 每次应用供应商时，用 `build_gateway_profile()` 重建的模板**整份覆盖** profile 文件（`src-tauri/src/claude_desktop_config.rs:1007` 的 `write_json_file(&paths.profile_path, &profile)`），它对自己不拥有的字段毫无概念，重建即丢弃。

同文件中其他所有写入点（`write_deployment_mode` L1100、`write_meta` L1140、`remove_cc_switch_enterprise_config` L1118）走的都是 `read_json_or_empty` → 改自己的键 → 写回的合并模式；**唯独 profile 走全量重建**。这是疏漏而非设计，也是为什么按字段逐个补（如 #7449 补 `autoModeEnabled`）治不了标。

三方围绕一个 last-writer-wins 的完整 JSON 文档博弈，平台（Anthropic）对「托管条目」没有合并契约：

```
CC Switch ──整份覆盖──┐
                      ├──> 3P profile 文件 <── 读 ── Claude Desktop
guard（取差值恢复）──┘        （last-writer-wins）
```

- **CC Switch**：写入方，替换式，只用 7 键模板 + 供应商路由字段
- **Claude Desktop**：保留式写入（只增改自己的字段），因此在这场冲突里必然输
- **guard**：无字段白名单的守护进程，持有「该文件里除 CC Switch 拥有的字段之外的全部内容」，以 overlay 取差值方式恢复；机制可靠，**问题在于它曾静默失联**

**两个必须区分的层次**（别把静态限制当 bug 修）：

1. **静态限制**——左下角显示 "Jason / Gateway" 而非期望的 "Chris"。这是 Claude 官方刻意设计：凭据为 static bearer key 时 `principalIdentity()` 为空函数 → 落到 `os.userInfo().username` 兜底。无任何配置键可改身份行，`PUT /api/account` 带 `full_name` 返回 403。**不要试图修这个。**
2. **动态事故**——配置改好后又回退。这才是本工作区要解决的问题。

---

## 修复方向

**上游合并语义（原推荐路径，现已被上游以别的方式实现，见下方「当前状态」）**：在上游 PR [farion1231/cc-switch#5417](https://github.com/farion1231/cc-switch/pull/5417) 落地——写入前先 `read_json_or_empty`，将自有键叠到旧对象上再写回。约 10 行改动，完全遵循同文件既有模式，MIT 协议。合并发版后升级即根治，**guard 可以卸载**。

已否决/降级的备选（不要重复提议）：

- ~~本地 fork 自编译~~——**该方案已于 2026-09-21 被采纳**（上游 PR 长期不合并 + 需要自有功能）。工具链已装好、构建与部署均已跑通；维护流程见 [UPSTREAM-SYNC.md](UPSTREAM-SYNC.md)。
- **Windows GPO 注册表托管配置**——代价是应用进入「受组织管理」状态、部分设置变只读、需写 HKLM，为一个显示名属过度设计。
- **补丁式方案（计划任务看门狗）**——**用户已明确拒绝**，已被上游 PR 路径取代。

**当前状态**（核实于 2026-09-30）：PR #5417 创建于 2026-07-15，**仍未合并**且已与 `main` 冲突；**但它的行为已由上游另一条路径实现** —— `81df5a08`（2026-09-26，"add a key-field write engine"）把配置写入改成按键打补丁，`gateway_profile_patch` 的 `clear` 只覆盖 `DESKTOP_PROFILE_FLOOR` 的 5 个网关键（`live/floor.rs:180`），其余 profile 字段原位保留。

因此：**当前已发的 `v3.20.4` 仍需补丁 A**（它早于该重构，整份覆盖仍在）；但**同步到含 `81df5a08` 的版本后可删补丁 A**（删前按 UPSTREAM-SYNC §7 复验：切供应商后 profile 非网关键存活）。已在上游 #5417 留言说明（其代码被取代、测试仍有价值）。核实命令：

```bash
gh pr view 5417 --repo farion1231/cc-switch --json state,mergeable,title,updatedAt
```

---

## 常用命令

### guard 守护进程

命令须在 guard 脚本目录下执行（状态文件存于脚本所在目录）：

```bash
cd "D:/Workspace/Claude Desktop/Code/Tmp/claude3p-profile-guard" && python guard.py --status
```

```bash
cd "D:/Workspace/Claude Desktop/Code/Tmp/claude3p-profile-guard" && python guard.py --show
```

```bash
cd "D:/Workspace/Claude Desktop/Code/Tmp/claude3p-profile-guard" && python guard.py --selftest
```

- `--status` 健康报告（自启键 / 进程 / 各目标状态 / 最近日志），末行 `RESULT: HEALTHY` 即正常
- `--show` 逐目标对照 overlay 与实时文件，列出不一致项；`!!` 表示外部又改了它（守护会在 2 秒内放回）
- `--selftest` 离线跑 10 项测试，**不动线上文件**——这是本工作区跑单个测试的方式
- 面板里改完设置后等约 20 秒（`USER_CONFIRM_S`）再 `--show` 确认 overlay 已收录
- 若编辑被拒（日志出现 `REFUSED`），用 `python guard.py --save` 显式固化

guard 依赖一个自动发现的默认目标；若要保护更多文件需在同目录写 `guard-targets.json`（当前该文件不存在，用的是默认目标）。

### 本地构建

工具链已就位：Rust 1.95 / cargo（**不在默认 PATH**，需 `export PATH="/c/Users/Jason/.cargo/bin:$PATH"`）、MSVC 链接器、node v24.15.0、pnpm 10.12.3、git 2.55.0、Python 3.14.4。

构建与部署的完整命令（含必需的 gh-proxy 镜像与代理绕行）见 [UPSTREAM-SYNC.md](UPSTREAM-SYNC.md) §5–§6。三条最易踩的：

- cargo 不在 PATH，先 export
- tauri CLI 下载 NSIS 工具必须走 `TAURI_BUNDLER_TOOLS_GITHUB_MIRROR` 且 `unset *_PROXY`
- 安装必须经 `tools/install-local.bat`（Git Bash 会转写 `/S` 与 `/D=`）

> **风险**：自建版会替换本机官方签名的二进制，而 CC Switch 持有全部供应商密钥并代理全部流量——仅本机自用，勿分发。

---

## 验证口径

**验收已通过（2026-09-21）**：3 次真实供应商切换后 profile 保持 22 键，Chris / Gateway、全部面板开关与用户新增设置均存活。以下口径保留，供上游同步后复验用：

1. 切换供应商 → 面板设置与左下角显示名不变（判据：profile 键数不骤降、非网关字段存活）；
2. 重启 Claude Desktop → 同上；
3. **覆盖事故的特征是键数骤降 + `deploymentDisplayName` 消失**——出现即回退。

对照基线：profile 的 CC Switch 自有键只有 7 个左右（网关地址/密钥/路由等），**其余全部是非网关字段**，任何一次供应商切换后都应原样存活。

---

## 历史记录：guard 侧（已废弃）

guard 已于 2026-09-20 卸载，根因（CC Switch 全量覆盖）也已被本地构建修复——**本节仅作历史备查，不再是排查方向**。

它曾暴露的教训值得记住：`pythonw` 无控制台，启动期任何错误静默丢弃、无日志可查，导致 9/20 事故的扳机是「guard 从未成功自启却无人发现」。若日后重新启用它（overlay 备份在同目录 `overlay-preserved-20260920.json`，`--install` 可恢复），务必先确认自启真的生效。
