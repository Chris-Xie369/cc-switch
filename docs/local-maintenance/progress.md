# SDD 进度账本 — cc-switch 3P profile 覆盖修复

计划: docs/superpowers/plans/2026-09-20-cc-switch-profile-merge-fix.md
分支: fix/profile-merge (基线 v3.20.3)
范围修正: 防覆盖改为「本地构建禁用 updater」(补丁 C)，原「靠签名天然防覆盖」说法已证伪。

---

## 现状（2026-09-21 收尾）

**项目已完成并上线。** 本地构建的 CC Switch 修复了 3P profile 覆盖缺陷，并含两个自有功能补丁，已部署、已验收。

| 项 | 值 |
|---|---|
| 分支 | `fix/profile-merge`（基线 tag `v3.20.3`），**已推送到自己的 fork** |
| 最新提交 | `dd02ba8a`（文档快照入库）；`origin/fix/profile-merge` 已同步 |
| 部署 | 3.20.3-local，`C:\Users\Jason\AppData\Local\Programs\CC Switch\`（构建于 09-21 14:50） |
| 验收 | 连续 3 次供应商切换后 profile **22 键无一丢失**（修复前：单次切换 19→7、显示名回退） |
| guard | 已卸载（2026-09-20 20:04）；overlay 备份在同目录，`--install` 可恢复 |
| 上游 | PR #5417 **仍未合并**；最新 release 仍是 `v3.20.3`；已在 PR 下留实测验证评论 |

**携带的补丁（同步上游时必须保留）**：A 合并语义 · B 显示名可配置 · C 本地构建 · D 上游状态检查。
完整清单（含各自的上游化状态）与同步流程见工作区 [`UPSTREAM-SYNC.md`](../../../UPSTREAM-SYNC.md)。

**待办**：无阻塞项。可选项：
1. 补一条守护「调用方必须剥离 `-local`/`v`」的测试（见「补丁 D 修复复审」节的 Minor）
2. 推动 PR #5417 合并（合并后可丢弃补丁 A）

**接手顺序**：工作区 `CLAUDE.md`（导航）→ 工作区 `UPSTREAM-SYNC.md`（维护流程）→ 本文件下方流水账（决策依据与踩坑细节）。

**文档备份**：工作区文档（playbook / 事故报告 / 技术简报 / 安装脚本 / 导航）已快照进 `src/docs/local-maintenance/` 并推送到 fork。
真实来源仍是工作区；改原件后需按该目录 README 的命令刷新快照。

---

## 任务状态（原始计划的 Task 1–8）
- Task 1: 工具链 — 完成（无代码变更，免审查；已核验 rustc/cargo 1.95.0 MSVC、pnpm 10.12.3、node_modules 就位）
  · 传递性注意事项：cargo 不在当前 shell PATH，后续 cargo 步骤需 `export PATH="/c/Users/Jason/.cargo/bin:$PATH"`
  · 环境事实：Windows SDK 装在 F:\Windows Kits（非默认盘）；仓库根有 rust-toolchain.toml
- Task 2: 补丁 A 合并语义 (cherry-pick PR #5417) — 完成（commit d03bf0cb，审查 ✅ Approved）
  · 已核实：远端无 fix/profile-merge 分支（未推送）；作者保留为上游 bobby；无 cherry-pick 残留
  · Minor 待裁（留待最终审查triage，不现在修）：read-before-write 使 profile 文件损坏时由「自愈覆盖」变为「硬失败」。
    不修的理由：本补丁刻意与上游 PR #5417 保持逐字节一致，本地分叉会变成需长期维护的差异。
  · spec 修正：§3.2 测试名由 claude_desktop_apply_* 改为 inject_display_settings_*（避免测试写用户真实设置文件）
- Task 3: 补丁 B 后端 (显示名三键 + TDD) — 完成（commit b0ba96de，审查 ✅ Approved，6/6 通过）
  · 注入点已核实：三键注入在 merge 之前的新 profile 上；None=不写（磁盘旧值由合并保留）
  · Default 字面量补 None 字段确属必需（settings.rs:535-584 为穷尽结构体字面量）
  · Minor 待最终审查 triage：
    (a) 缺 end-to-end 接线测试（None→磁盘三键存活路径），plan-mandated，接线正确性由验收第二步覆盖
    (b) 无 serde 序列化契约测试（claudeDesktopDisplay / omit-when-None），Task 4 前端消费该契约
    (c) claude_desktop_apply_* 测试现隐式假设 ambient settings 的 claudeDesktopDisplay==None
- Task 4: 补丁 B 前端 (设置项 UI) — 完成（commit f9874721，审查发现 Important 后修复闭合）
  · 审查 ❌→修复：空名字会写空串进 profile（plan-mandated）。用户裁定后端过滤空名字。
    fix commit 2b13804b：inject_display_settings 在 name 为空时 return；新增 inject_display_settings_omits_keys_when_name_empty。
    复审 ✅ Approved。剩余两处已知边界（非缺陷）：空白名字仍会写入；空名字无法表达「有意清空」。
- Task 5a: 补丁 C 禁用 updater + 版本号 + 全量测试 — 完成（commit e3695f0e + 418fa7e4，审查 ✅ Approved）
  · 配置正确：version→3.20.3-local 两处；updater/pubkey/endpoints/createUpdaterArtifacts 全移除；plugins.deep-link 保留；JSON 合法；Cargo.lock 同步
  · 测试：cargo test --no-fail-fast = 2950 过 / 12 失败 / 6 忽略；12 失败均预存环境性（symlink 1314、端口占用、真实用户配置断言），
    已在干净 v3.20.3 上复现一致；审查者确认无任何 Rust 测试读版本号/updater 配置，故与本补丁无关
  · 待办：Task 5b 构建 NSIS + pubkey 验证（控制器本会话执行）
- Task 5b: 构建 NSIS 安装包 — 完成（安装包已产出，pubkey 已验证移除）
  · 产物: src-tauri/target/release/bundle/nsis/CC Switch_3.20.3-local_x64-setup.exe (10MB)
  · pubkey 验证: 产物二进制 grep 官方 pubkey = 0（官方版 = 1）→ 补丁 C 生效
  · 环境要点（本机重建必读）: tauri CLI 下载 NSIS 工具被 gh-proxy 拒 400；
    须 `export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR=https://gh-proxy.com/https://github.com/`
    且 `unset HTTPS_PROXY HTTP_PROXY ALL_PROXY`（绕过 9674 代理直连 gh-proxy）
- Task 6: 部署 (退出/备份/覆盖安装) — 完成（2026-09-20 23:4x；后因补丁 D 于 09-21 再次部署）
- Task 7: 验收一 持久化 — 完成（2026-09-21 13:16–13:19，连续 3 次供应商切换，profile 22 键无一丢失）
- Task 8: 删除 guard + 验收二 可配置性 — 完成（guard 已于 09-20 20:04 被卸载，非本会话所为；可配置性随显示名功能上线并验收）
- Task D: 上游状态检查（计划外新增，详见下方专节） — 完成（`74665082` 实现 + `f115b91d` 修复，已部署运行）

## 整分支最终审查（opus）与收尾修复
- 结论：ready to deploy；序列化契约/注入顺序/空名字守卫/updater 移除/无附带损伤 全部核实通过
- 发现 1（Important，已修）：启用显示名只填名字时，空副标题会把磁盘 "Gateway" 抹成空串
  → 用户裁定「后端过滤空副标题」；fix commit 42c9983c（含 serde(default) 修发现 2）；复审 ✅ Approved
- 发现 2（Minor，已修）：ClaudeDesktopDisplaySettings 内层字段缺 serde(default)，残缺对象会重置全部设置 → 42c9983c
- 发现 3（Minor，决定不改）：profile 损坏时由自愈覆盖变硬失败 —— 保持与上游 PR #5417 逐字节一致
- 发现 4（Minor，已知边界）：官方供应商往返仍丢非网关键（§3.4 声明范围外）→ 验收时勿做官方往返
- 已知边界：空白则名字（含空格）仍会写入；空副标题/空名字无法表达「有意清空」

## Task 6 部署 — 完成（2026-09-20 23:4x）
- 安装到 C:\Users\Jason\AppData\Local\Programs\CC Switch\（用户原安装路径），运行中 3.20.3-local
- 校验：size 34326016（本地构建）、官方 pubkey=0（官方为 1）
- 官方 exe 备份：cc-switch.exe.official-3.20.3（34299392 字节）
- 用户原快捷方式（2026-03-19 创建）+ HKCU Run 自启项 均指向该路径 → 指向本地构建
- 注册表：HKCU\...\Uninstall\CC Switch = 3.20.3-local（我的安装）；
  HKLM\...\{1376663C-...} = 3.20.3（用户原 per-machine 安装记录，未动）

### 部署中踩到的坑（重要）
本会话运行在 Claude Desktop 的 MSIX 容器内，%LOCALAPPDATA% 被重定向到
...\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Local\...：
1) 首次静默安装（未指定目录）装到了默认的 %LOCALAPPDATA%\CC Switch（非现有安装目录），
   并创建了一个指向容器内副本的额外开始菜单快捷方式；
2) Git Bash 会吞掉/转写 /S、/D= 参数 → 必须经 .bat 调用安装器才能正确传 /D=；
3) 已清理：额外快捷方式、误装目录。现状已收敛干净。

## 剩余：Task 7/8 验收（需用户操作）
- Task 7 验收一（持久化）：需用户在 Claude Desktop Developer 面板改设置 + 在 CC Switch 切供应商
- Task 8：卸载 guard + 验收二（可配置性）

## 意外发现：guard 已被卸载（非本会话所为）
- 会话开始时 guard 为 HEALTHY（pid 17748、HKCU 自启项在）；现：进程消失、guard.pid 删除、自启项移除
- guard 目录新增 overlay-preserved-20260920.json（创建于 20:04，含 3p-profile 的 overlay 字段）→ 可恢复
- 本会话只调用过 guard.py --status（只读），未执行 --uninstall
- 影响：Task 8 的「卸载 guard」已无需执行；当前是验收的理想状态（CC Switch 3.20.3-local 为唯一写入者）
- 当前 profile 完好：19 键，deploymentDisplayName='Chris'、deploymentDisplaySubtitle='Gateway'、endUserAttribution=False

## 部署后追加：显示设置 UX 修复（用户在验收中发现）
- 现象：关掉再打开「管理显示设置」后，已填的显示名/副标题在表单里变空（磁盘数据未损，因后端空值=不写入）
- 用户裁定：保留上次输入 + 改提示文案
- 实现：commit 0fb72b4a（组件内 lastValues 记忆 + zh/en 提示改为「留空=保持现有值」）；仅 3 个前端文件
- 随后需重新构建 + 再次部署（合并语义代码未变，不影响已完成/进行中的验收结论）
- 另观察：profile 中 inferenceGatewayAuthScheme 消失，系 Claude Desktop 自身面板写入所致（CC Switch 自 09-20 19:35 后未写 profile）；网关工作正常

## UX 修复的审查与打磨
- 审查 commit 0fb72b4a：✅ Approved（无 Critical/Important）。核实了关/开往返路径、无渲染循环、
  关闭仍送 null、文件范围仅 3 个前端文件。
- Minor 1（已修）：提示文案易被读成「只跳过留空的字段」，与实际语义（显示名为空→三键全不写）不符
  → commit 20a63f49 精确化 zh/en 文案。
- Minor 2（未修，记录备查）：EMPTY_DISPLAY 为模块级常量并直接送入父 state；审查确认全仓无就地
  改写路径，无实际风险，故不改（surgical 原则）。
- 之后：重建（构建中）→ 部署 → 用户验收

## 二次部署（含 UX 修复）— 完成 2026-09-21 09:2x
- 构建：vite dist 09:02 → exe 09:25；dist 校验含新文案、旧文案已消失（二进制内前端资源经压缩，不能用 grep 明文校验）
- 安装：经 .bat 正确传入 /S 与 /D=，装入现有目录；exe size 34326016、pubkey=0
- 运行：pid 25924，ProductVersion 3.20.3-local
- profile：21 键完好（Chris / Gateway / false + builtinBrowserEnabled / skipWebFetchPreflight / sshHostAllowlist）
- 待办：用户在 CC Switch 切换供应商，验证非网关字段（含上列 3 键）全部存活 —— 决定性验收

## 磁盘清理（2026-09-21）
- 现象：仓库合计 27 GiB，其中 src-tauri/target/debug = 24 G（cargo test 的 debug profile 产物：
  deps 16G + incremental 7.9G + build 882M），release 仅 2.5G
- 单个大文件：debug/deps/cc_switch_lib.lib 1.94G、libcc_switch_lib.rlib 1.11G、多个 0.33-0.39G 的 .pdb
- 用户裁定：只删 target/debug（保留 release 缓存）
- 结果：仓库 27 GiB → 2.9 G；D 盘余量 29G → 53G；安装包与 release exe 完好，应用仍在运行
- 预防（若日后常跑测试）：CARGO_PROFILE_DEV_DEBUG=0 或 [profile.dev] debug=false 可省掉大部分 .pdb
- 操作坑：MSYS_NO_PATHCONV=1 时 cmd 要用 /c（单斜杠），用 //c 会进交互模式、命令不执行

## 验收 — 通过（2026-09-21 13:19）
- 用户于 13:16-13:19 做了 3 次供应商切换（三个不同 provider ID）
- 切换后 profile = 22 键（此前 21；第 4 个新键系用户在面板新增的设置，同样被保留）
- Chris / Gateway / endUserAttribution=false、autoMode/chatTab/toolSearch、
  builtinBrowserEnabled / skipWebFetchPreflight / sshHostAllowlist 全部存活
- 对比修复前：单次切换即 19→7、显示名回退、开关归零 → 修复确凿生效
- Task 6/7/8 全部完成；项目目标达成

## 补丁 D：应用内上游状态检查（2026-09-21 起）
- 背景：用户裁定「由 CC-Switch 自己完成检查和提醒」——它是开机自启的托盘应用，检查时机可靠，
  且提醒内聚在自包含产物里，不依赖外部会话/定时任务
- 设计：后端 check_upstream_status（查 PR #5417 merged + 最新 release）；复用既有 UpdateContext
  启动检查与 dismiss 机制；「关于」页不再尝试安装（本地构建装不了），改为引导同步 + 打开 release 页
- 代价：fork-only 功能，永久携带（补丁 D），须加入 UPSTREAM-SYNC.md 的补丁清单
- 网络前提已实测：api.github.com 在普通进程上下文中直接可达（9674 为系统级代理），无需额外配置
- 简报：.superpowers/sdd/task-D-brief.md

## 补丁 D 实现与审查（2026-09-21）
- 实现 commit 74665082（8 文件 +195/−70）；审查 ✅ Approved（无 Critical/Important）
  · 审查者核实：命令注册名与前端 invoke 匹配（tauri-macros 取路径最后一段）、User-Agent 已设、
    两个请求独立容错、安装路径已移除、双语 i18n 逐字一致、仅动 8 个文件
  · 实现者对简报的三处偏离均属改进，其中一处修复了简报本身的陈旧值 bug（useCallback [] 依赖）
- 复用性缺陷（简报导致，审查者与我均漏过）：lib/version.ts 早有 tested 的 compareVersions/
  isUpdateAvailable 且 AboutSection.tsx 在用，简报却让其另写两套比较逻辑
  · 关键陷阱：semver 中预发布 < 正式，故 isUpdateAvailable("3.20.3-local","3.20.3") = true →
    同号基线被永久误报为「有新版」。必须剥掉 -local 与 v 前缀后再比
- 修复 commit f115b91d（4 文件 +28/−68，净减少）：
  · updater.ts 改用 isUpdateAvailable + 基线归一化；Rust 比较函数与测试整体删除（无生产调用方）
  · UpdateBadge 在「仅 PR 合并」态改显示 upstreamPrMerged，不再误报「检测到新版本：3.20.3-local」
  · version.test.ts 增测试钉死该陷阱
- 验证：cargo exit 0 / typecheck exit 0 / version.test.ts 9/9
  · 全量 test:unit 1102 过 5 失败，已用 stash 基线证明为预存环境性
- 待办：复审 → 构建（进行中）→ 部署

## 补丁 D 修复复审（2026-09-21）
- 复审 ✅ Approved（commit f115b91d）。逐值追踪确认误报路径消除、真实更新仍可检出、
  Rust 删除无悬挂引用、4 文件范围准确，未引入新缺陷
- 审查者补充的关键发现（对称风险）：两处剥离都承重 ——
  · 不剥 -local → semver 判 3.20.3-local < 3.20.3 → 同号基线永久误报
  · 不剥 v    → parseVersion 返回 null → compareVersions 返回 0 → 真实新版被静默漏报
  两种漏法方向相反，但都是「永远不提醒」
- Minor 待最终审查 triage：新增测试只钉住 version.ts 的语义，**不能**守护调用方
  「必须剥离 -local/v」的义务 —— 删掉 updater.ts 的 replace 该测试仍全绿。属未来维护风险，
  非当前缺陷；现在改动会作废正在进行的构建，故记录不修

## 补丁 D 部署与验证（2026-09-21 14:5x）
- 构建 14:50（34349056 字节）→ 经 tools/install-local.bat 装入现有目录 → 大小匹配、pubkey=0
- 运行：pid 32384，3.20.3-local
- API 端到端实测（用与应用相同的网络条件，不带 shell 环境变量代理）：
  · pulls/5417 → merged=false (bool)、state=open  ← 与代码读取字段一致
  · releases/latest → tag_name='v3.20.3'、非预发布/草稿
  · 按前端逻辑比较：基线 3.20.3 vs 上游 v3.20.3 → 无更新 ✓（正是修复要消除的误报场景）
- 当前无提醒属正确行为（PR 未合并、无新版本）
- 补丁 D 已加入 UPSTREAM-SYNC.md 补丁清单（永久携带，每次同步需保留）

## 收尾：磁盘清理（第二次）与文档备份（2026-09-21 21:xx）
- 磁盘：`cargo test`（Task D 的验证跑了几轮）又生成 target/debug 15 G → 已删；仓库 18 G → 2.9 G，D 盘余量 39 G → 53 G
  · 已把两种应对写进 UPSTREAM-SYNC.md §5：跑完测试删 debug 树，或直接用 `cargo test --release` 不生成 debug 树
- 文档备份：工作区文档原先只在本地磁盘（代码有 fork 备份而它们没有）
  · 快照进 `src/docs/local-maintenance/`（commit dd02ba8a）并推送到 fork
  · 工作区 CLAUDE.md 的 `CLAUDE.md` 在快照中改名为 `WORKSPACE-NOTES.md`——
    上游 .gitignore 第 10 行忽略 CLAUDE.md，改名避免与上游约定冲突
  · 工作区 CLAUDE.md 资产表补一行，提醒「改原件后需刷新快照」
- 顺手修了一处失效链接：两份分析报告已被移入工作区 `doc/`，CLAUDE.md 内的链接仍指旧路径

## 探针验证：自定义后缀模型 ID 是否被 Claude Desktop 接受（2026-09-22）
背景：多供应商共存方案需要把供应商标识编进模型 ID（如 claude-sonnet-glm）。约束是 Claude Desktop
1.12603.1+ 的 fail-all 校验器要求 ID 形如 claude-{sonnet|opus|haiku|fable}-{标识}；CC Switch 代码
只对角色前缀做校验、后缀不管，但"角色前缀+自定义后缀"是否真被 Claude 接受**无人验证过**。
决定：若能接受 → 槽位无上限，真正做到"全部模型上线"；若拒绝 → 退回官方 ID 的 9 槽方案。

操作：向 profile 的 inferenceModels 追加探针条目（4 → 5 条）
  {"labelOverride": "PROBE自定义ID", "name": "claude-sonnet-probe", "supports1m": true}
备份：%LOCALAPPDATA%\Claude-3p\configLibrary\pre-probe-backup-20260922_102856.json
回滚：tools\revert-probe.bat（双击即可，恢复后需再重启 Claude Desktop）

判据：
  · 模型列表显示 5 条（含 PROBE自定义ID）→ 自定义后缀被接受 → 走无上限方案
  · 模型列表变空/异常 → fail-all 生效 → 回滚，走 9 槽方案
状态：探针已写入；等用户重启 Claude Desktop 后报告现象。

### 探针验证结果：通过 ✅（2026-09-22）
现象：重启 Claude Desktop 后模型选择器显示 10 项；其中两项为 `PROBE自定义ID` 与 `PROBE自定义ID 1M`。
结论：**自定义后缀模型 ID 被 Claude Desktop 完全接受**（无 fail-all 拒收）→ 槽位无上限，
      可真正做到"全部已启用供应商的模型上线"。

附带发现（重要，影响设计）：**supports1m: true 会让每个模型在选择器里渲染成两行**
  （本体 + "1M context window" 变体）。10 = 声明 5 条 × 2，与 profile 完全吻合。
  → 设计含义：① 槽位比预期更充裕；② supports1m 需按需使用，否则列表条目翻倍。
描述文字（如 "Most efficient for everyday tasks"）来自目录里对应角色的元数据，非我们控制。

遗留：探针条目仍在 profile 与模型列表中（无害的测试残留），可在下次自然重启时清除。

## 新特性：Claude Desktop 多供应商共存（聚合供应商）— 设计+计划完成（2026-09-22）
需求：一次配置多家 → 重启一次 → 在 Claude 模型列表里自由选任意一家的模型（选择权归用户）。
调研：Desktop 侧无现成实现；上游 #5937（+5195 行，CI 全绿，等 review 近两月）覆盖的是
      Claude Code CLI + Codex，其「聚合供应商」设计可直接借鉴。社区诉求极高（#3703 血书）。
平台约束（实测）：模型 ID 须 claude-{角色}-{标识}；**自定义后缀被接受、无 fail-all**（探针验证）
      → 槽位无上限；supports1m 会使每个模型在选择器里双行渲染；profile 只有单个 gateway 端点。
方案：聚合供应商（无端点无凭据，路由表挂 meta.aggregateRoutes）；按模型 ID 查表换成目标供应商 +
      改写 body.model；未命中走默认目标。**不改「启用」的全局语义**（一个聚合供应商即等价于多家在线）。
产物：spec `docs/superpowers/specs/2026-09-22-claude-desktop-multi-provider-design.md`（13 节）
      plan `docs/superpowers/plans/2026-09-22-claude-desktop-multi-provider.md`（8 个任务）
计划自检修正两处：① 槽位 ID 改为生成后持久化（否则 slug 退化为 id）；② 默认目标按槽位 ID 而非下标引用。
待办：执行计划（Task 1–8：数据模型 → ProviderMeta → 模型列表派生 → 请求路由 → 校验 → 前端类型 → UI → 构建部署验收）。

---

# 新计划：聚合供应商实施（2026-09-22 起）

计划: docs/superpowers/plans/2026-09-22-claude-desktop-multi-provider.md（8 个任务）
Baseline: fa65a311（预检修正后）
⚠️ 注意：本节的 Task 1–8 与上方旧计划的 Task 1–8 **不是同一批**，勿混淆。
   简报文件 .superpowers/sdd/task-N-brief.md 已被本计划覆盖。
   报告文件用 task-agg-N-report.md 以区分旧计划的 task-N-report.md。

## 聚合供应商 任务状态
- Task 1: 槽位数据模型与 ID 生成（aggregate.rs）— 完成（commit 750e1a34，审查 ✅ Approved）
  · 实现者发现并上报了一处**设计级错误**（未擅自绕过）：文档/测试称「中文名一律回落为 id 前缀」
    并举例「智谱 GLM」，但 GLM 本身是 ASCII → 实际 slug 为 glm，回落分支不触发。
    该错误同时存在于设计文档 §5、计划的 Rust 测试与 TS 实现（Task 6 会撞同一面墙）。
  · 裁定方案 A：规则不变（混合名取 ASCII 部分是对的），修正文档示例与测试
    → 计划 4 处 + 设计文档 1 处修正，commit fc787d8b
  · 遗留：7 个 dead_code 警告（模块尚未被消费，预期过渡态，未用 allow 压制）
  · 审查结论：✅ Approved，逐值追踪了 5 个输入、领先分隔符守卫、截断/去尾顺序、
    安全 ID 保证与去重确定性，全部核实通过
  · 审查发现 2 个 Important 覆盖缺口（代码正确、缺测试守护）→ **已补测闭合**（commit 576d8471，8/8 通过）：
    ① 截断落在分隔符上（唯一能区分「先截断再清理」与「先清理再截断」的输入）
    ② 去重跳过已存在的编号（taken=[base, base-2] → base-3）
  · Minor 记录备查：
    (a) slugify 的 id 回落分支不对 provider_id 做净化 —— 空 provider_id 会产出
        `claude-sonnet-`（不安全）。已核实：Task 5 的校验会先以「槽位必须同时指定
        目标供应商与上游模型」拦下空 providerId，该缺口有清晰错误信息兜底，无需改
    (b) 文档注释说「纯非 ASCII」才回落，实际是「不含任何 ASCII 字母数字」（纯标点也回落）——
        措辞不精确，无行为差异
    (c) 审查者指出：结尾带 '-' 的 ID 其实仍能通过 is_claude_safe_model_id（尾段非空即可），
        故「先截断再去尾」是防御性而非必需 —— 记录以免后续任务过度解读
- Task 2: 挂到 ProviderMeta — 完成（commit f7d12f57，审查 ✅ Approved）
  · 接线信号已实测：把 provider.rs 回退到基线重编，dead_code 7 → 2；
    消失的 5 个正是 AggregateTier/as_str/AggregateRouteSlot/DefaultTarget/AggregateRoutes
    （剩余 2 个是 slugify/generate_slot_id，待保存路径调用时消除）→ 字段确实被引用
  · 实现者第二次发现计划错误：Task 2 测试 JSON 漏了必填的 routeId → 反序列化报
    missing field。裁定方案 A（route_id 设计上必填，错的是测试）→ 计划已修（b9b39b05）
  · 两个失败测试已由控制器实测确证为环境性：
    ① update_current_claude_desktop_provider_syncs_profile_when_proxy_takeover_is_active
       —— 报 os error 10048（地址已占用），因本机 CC Switch 在跑，端口被占
    ② import_hermes_providers_from_live_updates_existing_provider_from_live —— 读真实 live 配置
    两者均属旧计划 Task 5a 记录的 12 个预存失败之列，与新字段无关
  · 审查核实：字段 serde 形状与邻字段一致；None 遗漏断言会拒绝 `null`（强于仅查键不存在）；
    DefaultTarget 的 tagged 表示被往返覆盖；抽样 100+ 个 ProviderMeta 字面量构造点，
    全部用 ..Default::default()，加字段不破坏任何一处
  · Minor 待最终审查 triage（均为简报原文，非实现缺陷）：
    (a) 测试名 round_trips 但未做反向序列化（序列化 Some 的一侧无断言）
    (b) 输入 JSON 里的 label 未断言
- Task 3: 模型列表派生（profile + /models）— 完成（commit b97368bd + a31ab52e，审查 ✅ Approved、修复复审 ✅ Approved）
  · 实现者查明：`model_list_response` 首行即委托 `proxy_model_routes`，**无独立路径**
    → 未插入第二处分支（计划里「两处都插」属冗余描述，已避免重复）
  · 测试：aggregate 14 passed；claude_desktop 50 passed / 1 failed（已知环境性端口占用）
  · dead_code 2 → 2，无新增
  · 具名风险（控制器取证后交审查判定）：聚合路径不校验 route_id 的 claude-safe 性，
    而普通路径有 repaired_route_id 修复逻辑（claude_desktop_config.rs:596）。
    若触发，后果是 Claude Desktop **拒收整组模型**（已实测的 fail-all 行为）
  · 审查判定：真实缺陷、Important（非 Critical，因当前写入方只产安全 ID）。审查者明确反驳了
    「上游校验会拦」的豁免理由：可达性依赖「数据永远由本程序生成且永不被编辑」，而同目录其它
    配置正是靠手工/导入流转。并指出应 **filter 而非 repair** —— route_id 是稳定路由键，
    repair 会让选择器显示的 ID 与路由表的键错位、查表落空、行为不可预测
  · 另 3 处 Minor 一并修：缺 dedup_by(route_id)；空结果返回 Ok(vec![]) 而非 Err（会安静写出空
    inferenceModels → 模型选择器空白）；use 语句置于文件末尾不合项目惯例
  · ⚠️ 修复引入的新交互（须传给 Task 4）：filter 后槽位可能被丢弃 → 若 default_target 以
    SlotId 引用被丢弃的槽位，该引用会悬空。Task 4 的 resolve_target 目前对此返回明确错误
    （「默认目标指向了不存在的槽位」）——可诊断、不静默误路由，保持该行为即可
- Task 4: 请求路由（按模型换供应商）— 完成（commit 5ce43e14 + c991e2d3，审查 Needs fixes → 修复 → 复审 ✅ Approved）
  · 测试：aggregate 18/18、proxy 1475/1475、claude_desktop 50+1（已知环境性端口占用）；dead_code 2→2
  · 计划缺陷 #A（严重）：计划称「在 map_proxy_request_model 之前改写 model」→ 会打断每个
    Claude Desktop 请求：映射器会拿已改写的上游模型名去查目标供应商路由表 → route_unknown/
    routes_missing。正确做法是**替换**该映射（聚合路由已知上游模型，无需再映射）
  · 计划缺陷 #B：只替换 provider 不够 —— 转发器遍历 providers 列表，providers[0] 仍是
    无端点的聚合供应商，必须一并换掉
  · 计划缺陷 #C（语义问题，交审查判定）：成功后的故障转移同步会把「当前供应商」切成目标
    供应商 → 聚合只生效一次。实现者用「把 current_provider_id 指向目标」压住，但主动要求
    审查确认 identity-vs-routing 语义是否正确、有无副作用（该字段还有别的消费者）
  · 改写落点：RequestForwarder.aggregate_override 字段（经 with_aggregate_override 注入），
    在 forward() 内生效；未改动任何 forward_with_retry 调用点；Claude Desktop 路径为 handlers.rs:206
- Task 5: 保存校验与删除保护 — 完成（commit 5847b834 + e907fbd7 + 1bb10d68 + 5a40787c，审查与两轮复审均 ✅ Approved）
  · 落点：删除保护在 ProviderService::delete（新增 reject_if_referenced_by_aggregate）；
    禁嵌套在 DB 感知的保存层（add/update，新增 validate_aggregate_not_nested）
  · 测试：aggregate 30/30、provider_service 43/43、provider:: 175 过 2 失败
    （两个都属环境性/预存：10048 端口占用；hermes 那个已用 stash 干净树对照确认同样失败）
  · 计划缺陷：① 测试用 settings_config:{} 构造供应商 → 会在聚合校验**之前**被直连校验拦下，
    4 条断言里 3 条无法通过（实现者改用合法直连配置以保持非空洞）
    ② validate_provider_settings 是关联函数（ProviderService::）而非裸函数
    ③ db.get_providers 实为 db.get_all_providers
  · ⚠️ 死代码判定（审查者调查后定论 = **删除**）：全仓无生产调用点；mod aggregate 私有故 pub fn 不可达
    （正是那 2 条警告）；**关键**：无任何导入路径需要后端生成 ID —— deeplink 走 ProviderService::add
    （会校验并拒绝坏 ID）、import_config_from_file 是整库 SQL 还原（ID 原样带回）。导入要么带回原 ID、
    要么是畸形数据，**明确报错优于静默编造 ID** → 控制器此前提的「导入体验」顾虑不成立
  · Minor（判定为规格符合性缺口，已纳入修复）：**禁嵌套有单向漏洞** —— 检查只在保存聚合供应商时执行，
    故「普通供应商先被引用、之后被改造成聚合」会放行 A→B 嵌套，而设计 §9 明确禁止该状态
  · Minor（记录不修）：部分导入路径（import_claude_desktop_providers_from_claude、整库导入）不经
    add/update 故不经聚合校验 —— 与仓库既有校验的生效面一致，属既有行为
  · 实现者顾虑：保存时允许槽位目标供应商尚不存在（存在性延到运行时 resolve_target 兜底）
- Task 6: 前端类型与 ID 生成 — 完成（commit 8a99b23c，审查 ✅ Approved）
  · 两条转移过来的不变式已重建且**经变异验证**（破坏版会失败）：
    截断落在分隔符上、去重跳过已占用编号
  · 计划缺陷 #6：简报的测试 import 漏了 assignSlotIds（照抄无法编译）
  · 实现者记录的工具链事实：`pnpm test:unit -- <file>` **不真正过滤**，须用 `npx vitest run <file>`
    （此前多个任务的派发指令用了前者，实际是跑全量）
  · 已知非阻塞：全量套件有 3 个 PiProviderForm 超时（与本次无关）；空 providerId 会产出
    `claude-{tier}-`（不合规），但 Task 5 的校验会先以「槽位必填目标供应商」拦下
- Task 7: 配置界面 — 完成（df76af90 + 03267ced + 2b701481 + 543c90cb，三轮修复后复审 ✅ Approved）
  · 接入方式：在表单 !isOfficial 分支顶部加独立开关（未复用 direct/proxy 模式选择——那个值贯穿大量下游逻辑）
    开启→初始化 {slots:[], defaultTarget}；关闭→**delete meta.aggregateRoutes**（含官方分支）
  · candidates = useProvidersQuery('claude-desktop') 过滤掉自身与聚合供应商
  · 输入组件：显示名用 **ImeSafeInput**（该字段预期填中文，沿用仓库惯例）；upstreamModel 用普通 Input
  · 验证：typecheck 干净、vitest 13/13、prettier 干净；全量 136/138 文件过（PiProviderForm/App 仅并行负载下超时，
    隔离跑 53/53 通过，属预存 flaky）
  · ⚠️ **设计矛盾（Important，已派发修复）**：`validate_direct_provider`（claude_desktop_config.rs:338）只对
    官方供应商短路，**无聚合短路** → 创建聚合供应商必须编造占位端点与密钥，与设计「无端点无凭据」直接矛盾。
    实现者正确地**没有**只放宽前端（服务端仍会拒），故修法是后端补短路 + 前端相应放宽
  · Minor（已纳入同一修复）：canSaveAggregateRoutes 是死导出；候选目标未排除官方供应商（其无网关凭据）
- Task 8: 构建、部署与验收 — 未开始

- Task 1 最终状态：完成（750e1a34 + 576d8471）。补测由控制器直接核对 diff（21 行纯测试、内容逐字给定），未再派审查

- Task 3 最终状态：完成。修复复审确认四项发现全解、三个新测试非空洞（撤销修复即失败）

## Task 4 审查结论（opus 审查者）与修复
判定：**Needs fixes**（2 Important）。三处计划缺陷的判断经复核全部成立。

### Important 1：故障转移链未截断 → 模型名串台 + #C 抑制失效
- 聚合分支只换 providers[0]，链上其余成员保留，而 aggregate_override 是**请求级**字段
- 触发条件：故障转移开启 + 聚合在队列首位 + 队列还有别家 + 槽位目标失败
- 后果：(a) 后续供应商收到**槽位的上游模型名**（如 glm-5.3）且绕过它自己的路由；
  (b) 它若成功则 should_switch 判真 → **把聚合供应商切走**，换成既非聚合也非槽位目标的
  一家 → 聚合静默失效（比原缺陷更糟）
- 修正：把链截断为目标（一行）。同时落实设计 §6「聚合层不引入自己的故障转移策略」，
  并使 #C 的抑制**无条件成立**（而非仅链首成功时）
- 备选（未采用，属超范围）：显式 is_aggregate_routed 标志 + per-provider override；
  若要保留「同模型多渠道故障转移」才需要它

### Important 2：核心正确性无测试兜底
- 现有测试只断言 resolve_target 的命中/未命中；**无任何测试验证槽位上游模型名到达出站 body**，
  也未断言 providers[0]/current_provider_id 被换（恰是 #B/#C 最易回归处）
- 实现者称「需要活动上游」→ 审查者指出辩解不成立：仓库已有现成端到端夹具
  （proxy/server.rs:620-740，TcpListener + axum mock 上游捕获 body）+ Database::memory()
  + set_current_provider + 真 ProxyServer + reqwest

### 审查确认可用的结论（供后续参考）
- #A（改写代替映射）判断正确且必改：先改写会让映射器拿已改写的名字查目标路由表 → 必然失败
- #B（必须同时换 providers[0]）判断与实现完整；请求期内「供应商」只存在于两处字段，均已替换
- #C 的机制可接受（等价于聚合路由标志），但需配合链截断才完备
- `current_provider_id` 全仓只有一个判断点（should_switch），无计费/日志/账号边界消费者 —— 已核实
- ⚠️ **Task 8 验收注意**：代理面板的 `active_targets`（server.rs:265-274）会显示**目标供应商**，
  那是「实际在服务的供应商」诊断位，**不是缺陷**，不要误判为「当前供应商被切走了」

### Minor（记录）
1. 跳过 map_proxy_request_model 会丢掉其尾部的 Mimo 专用副作用
   （normalize_mimo_anthropic_thinking_history，全仓唯一调用点）→ 同一目标供应商在聚合/非聚合
   两种入口下行为不一致。窄（仅 Mimo），用户当前未用 Mimo，**记录不改**
2. 路由分支未按 AppType 收口（handler_context 只判 meta，forwarder 只对 ClaudeDesktop 应用 override）
   → 已纳入修复
3. resolve_target 两条错误路径无测试 → 已纳入修复
4. 未命中走默认目标时模型名仍交给目标自己的路由表 → brief 刻意要求，属预期行为

### Task 4 修复结果（commit c991e2d3）与一处定性纠正
- **端到端测试已落地并断言了核心**：`aggregate_routes_rewrite_model_and_use_target_provider`
  驱动真 ProxyServer + axum mock 上游（经 /claude-desktop/v1/messages）：
  · 命中：mock 收到的 `body.model == "glm-5.3"`（槽位上游模型）+ `Authorization == "Bearer target-secret"`（目标凭据）
  · 未命中：仍到达默认目标且 model 未被改写
  · 另断言 `db.get_current_provider == "agg"`、`failover_count == 0`（§4 不变量）
- Findings 3（AppType 收口）、4（两条错误路径测试）已完成
- **定性纠正（控制器已独立核实）**：Important #1 的触发条件**对 claude-desktop 不可达**——
  `proxy_config` 有 `CHECK (app_type IN ('claude','codex','gemini','grokbuild'))`（schema.rs:127/883/1445），
  claude-desktop 不在其中，故障转移结构上无法为该 app 开启 → 链长恒为 1。
  故 `providers.truncate(1)` 是**防御性**改动（今天是空操作，若将来放宽该 CHECK 才生效），
  非活跃缺陷修复。审查者评估可达性时未查到这条结构约束。
- **修复者的良好判断**：它为链截断尝试写测试，验证后发现**删掉修复行测试仍通过**（无法区分），
  于是**删除了这个会制造虚假信心的测试**，而不是留着充数

- Task 4 最终状态：完成。复审确认 e2e 测试非空洞（变异分析：空操作/整段移除两种改法都会失败）、
  两条错误路径测试有区分力、AppType 收口与链截断均未引入缺陷

- Task 5 最终状态：完成。复审确认禁嵌套反向扫描位于「自身有 aggregate_routes」分支内，
  四个合法场景（新建普通/被引用者仍为普通/聚合指向普通/重存无人引用）全部仍成功；新测试非空洞
  · 复审追加发现：`AggregateTier::as_str` 亦为死代码，但**逃过 dead_code 警告** —— rustc 不报告
    未使用的固有方法（审查者实验证实）。故「警告 2→0」只说明 rustc 报了什么，不等于无死代码残留
  · 另两处注释（aggregate.rs:266/338）仍称「槽位 ID 在保存时由后端生成」，删生成器后失真 → 一并清理

### 工具链坑（记录备查）：task-brief 的 infence 守卫
- `task-brief` 脚本用 awk 实现，含 `!infence` 守卫 —— **代码块内的 `### Task N` 标题会被忽略**。
- 因此计划文件里只要有**一个未配对的 ``` **，其后所有任务都无法提取：
  · 表现为「task N not found」，或**简报过度提取到文件末尾**（Task 5 那次拿到 799 行，含 Task 6/7/8）
- 本次成因：Task 5 段落内一处早先编辑遗留的孤立 ```（同时也会让 markdown 渲染错乱）
- 排查手法（可复用）：逐 Task 标题统计 ``` 出现次数，奇偶不配对处即失衡点
  ```python
  # 截至每个 "^### Task" 行统计以 ``` 开头的行数，奇数即未闭合
  ```
- **教训**：编辑计划文件后，若 task-brief 行为异常（找不到/过度提取），先查围栏配对，别怀疑任务编号

- Task 5 收尾清理：5a40787c 删除 AggregateTier::as_str（grep 证实无调用点）并修正两处失真注释。
  dead_code 0。**Task 5 完全闭合。**
  ⚠️ 传给 Task 6 的要点：随生成器一并删除的 Rust 测试里，有两条是 Task 1 审查时特意补的覆盖缺口
     （① 截断落在分隔符上——唯一能区分「先截断再清理」与「先清理再截断」的输入；
      ② 去重跳过已存在的编号 taken=[base, base-2] → base-3）。**必须在 TS 侧重建**，否则无守护。
     另：TS 侧现为槽位 ID 生成的**唯一实现**（Rust 版已删），计划里「与 Rust 逐字等价」已无对象。

- Task 6 审查核实：类型逐字段对齐后端 serde；算法轨迹经独立模拟复现；
  两条不变式经变异验证确认为非空洞（19 vs 20 字符截断、`-2` vs `-3` 去重）
  · Minor（记录，均已被其它层闭合）：空 providerId 产出不合规 ID（Task 5 校验先拦）；
    回落路径不转小写（应用 ID 为小写十六进制，理论问题）；三个工具函数暂无测试（属 Task 7）
  · **Task 6 闭合**

### Task 7 修复结果（03267ced）与两点重要发现
- **Finding 1 实为三处后端改动**（实现者发现，比预估严重）：只改 validate_direct_provider 会留下
  仍在拒绝聚合的活路径 —— 还需 validate_proxy_provider 短路 + apply 钉死代理分支。
  单改一处 = 假修复。这正是派发时要求「先验证无路径依赖聚合自身端点/凭据」的价值
- 无路径使用聚合自身端点/凭据（已验证）：运行时派发前 provider 被目标整体替换；
  extract_base_url/extract_credentials 对聚合已无活路径输入
- 前端：两处必填校验加 `&& !aggregateRoutes`，聚合模式下 env 不写 base_url/key；普通供应商逐字未变
- canSaveAggregateRoutes **接线未删**：在槽位编辑器表头下渲染内联提示（后端 toast 仍为硬门禁）
- **教科书级基线反证**：把 claude_desktop_config.rs 按 md5 逐字节还原为 HEAD 后复现同样 11 项失败
  （2845 通过）→ 证明为预存问题。根因：15+ 模块共享进程级 CC_SWITCH_TEST_HOME、跨模块无锁的临时 home 竞态
- 未收口项（已派发补）：聚合与「直连」模式选择的互斥 —— 服务端已钉死代理，界面却仍可同选 → UI 与落盘不一致

### 值得单独立项的观察（记录）
- 跨模块共享 CC_SWITCH_TEST_HOME 导致的测试竞态：全量 lib 测试有约 11 项因并行 home 竞态失败，
  与业务代码无关。**建议单独立项**（属测试基础设施，不应混在本特性里修）

### Task 7 全链审查结论（opus 审查者，3 个 Important）
- **(A) 后端放宽经独立追证：安全且完整，三处缺一不可** —— 审查者逐条追踪了「无活路径消费聚合自身
  端点/凭据」，结论成立（替换点在 handler_context，抢在所有提取之前；models 端点只读槽位；
  map_proxy_request_model 仅在未命中时调用；direct_gateway_credentials 改动后仅剩两个调用点，
  其一已短路、其二失败即 None 无害）。并复核了实现者「不能改 provider_mode」的反例成立。
- **(B) 关闭态等价性逐分支成立**（8 处改动条件都比对了「普通供应商取哪一支」）；
  另核实 ProviderMeta.aggregate_routes 带 skip_serializing_if，普通供应商永不序列化该键
  → `aggregateRoutes !== undefined` 不会误判
- **Important 1（真问题）**：`default_target` 前后端**均无校验**。两条可复现路径：
  (a) 加了槽位但从未碰默认目标下拉 → `{kind:"providerId", value:""}`；
  (b) 选定 slotId 型默认目标后改动档位/增删槽位 → assignSlotIds 重编号 → 引用悬空或静默改指。
  后果：未命中槽位的请求（**Claude 内部调用正走此路径**）在运行时硬失败。
  设计 §8/§9 明确要求保存时拦截。前端的 canSaveAggregateRoutes 只判 Boolean(defaultTarget)
  （空串恒真）故提示不出现；后端全树无此校验（grep 确认）
- **Important 2**：commit 3 强制 proxy 后，聚合仍渲染**代理模式的四档模型映射区块**（带命令式文案
  「为 Sonnet、Opus、Haiku 三档分别填写…」），但提交时被 delete 丢弃 → 用户有充分理由认为必须填。
  **属本 diff 新引入**（强制 proxy 之前不会渲染它），应本轮收口
- **Important 3**：最承重的**代理短路无测试** —— 新增测试构造的是 mode: None（=Direct），
  验证的是直连短路 + 钉死；删掉代理短路不会有任何测试失败，而它才是主路径的唯一放行阀
- Minor（记录，不修）：空候选时「添加槽位」会产出必然非法的 ID（预览展示非法值）；
  is_compatible_direct_provider 对聚合返回 true（今日不可达）；get_status 用未钉死的 mode
- **流程注意（审查者提出，控制器采纳）**：03267ced 改了 `claude_desktop_config.rs`，
  超出 Task 7 逐字列出的文件范围。理由充分（设计 §4 要求，不改则功能不可用）且属评审提出的修复项，
  但**需记入补丁清单** —— 待 Task 8 完成时一并更新 UPSTREAM-SYNC.md 的补丁 E 描述

- Task 7 最终状态：**完成**。第三轮复审逐项确认：
  · 后端 default_target 校验非空洞 —— 审查者验证「空 ProviderId」用例不会被任何既有检查顺手拦下
    （故删掉新校验该测试必失败），并确认槽位本身合法、聚合走 validate_direct_provider 早退
  · 重编号重映射**按槽位位置**跟随（对象引用定位 + 位置回退），槽位被删时留悬空（不静默改指），
    providerId 型目标完全不受影响
  · !isAggregate 收口正确且不困住用户：被隐藏的块对聚合本就是派生数据；apiFormat 仍为可接受的默认值；
    两个校验器都豁免聚合的端点要求
  · 新增代理短路测试是真对照（两例都直接调 validate_proxy_provider；删短路则正例失败、
    删凭据检查则对照例失败）
  · 无回归：唯一共享的生产改动对非聚合供应商恒为惰性

## Task 8 步骤 1–2：全量测试与前端检查（2026-09-22）
- `cargo test --lib --no-fail-fast` = **2853 passed / 11 failed / 6 ignored**
- **11 项逐条归因（无一是本次改动引入）**：
  · model_pricing × 5 —— 真实用户配置断言（left:4/right:1 类）
  · import_hermes_... —— 读真实 live 配置（left:5/right:1）
  · update_current_claude_desktop_provider_... —— os error 10048（端口被运行中的 CC Switch 占用）
  · session_usage_grokbuild / codex_config 各一 —— Os code 1314（符号链接权限）
  · commands::misc::build_tool_search_paths_... —— 路径相关
  · **validate_aggregate_rejects_ordinary_provider_becoming_referenced_aggregate** —— 我们的测试！
- ⚠️ 该测试的失败**不能只看「11 = 11」就放过**（数字相同不证明是同一批）。控制器实测：
  · 隔离单线程运行 → **ok**（1 passed）
  · 并行运行下的 panic = 「原子替换失败: …\.tmpKDMaFR\AppData\Local\Claude\claude_desktop_config.json」
    —— 失败发生在测试**第一步**（保存普通供应商），**尚未走到聚合断言**
  · 结论：被既有 CC_SWITCH_TEST_HOME 跨模块竞态带崩，非逻辑问题；隔离即过
- **反证**：数字相同但成员可能被替换 —— 这次虽然确认是同一类问题，但排查动作本身是必要的
- 教训（已记入前述「值得单独立项」）：该测试基础设施缺陷会让**新加的测试也变成 flaky**，
  从而在未来掩盖真实的逻辑回归

## Task 8 步骤 3–4：构建与部署（2026-09-22 14:30）
- 构建：vite 18s + cargo release 14m50s → `CC Switch_3.20.3-local_x64-setup.exe`
- 产物 exe 34438144 字节；**pubkey = 0**（补丁 C 仍生效）
- 部署：经 tools/install-local.bat 装入现有目录 → 大小匹配、pubkey=0
- 运行：pid 2648，ProductVersion 3.20.3-local
- **前端新 UI 已确认随产物发布**（dist 中可检出 aggregate.title / aggregate.enable /
  notSaveable / modeLockedHint）——避免「后端改了但前端没打进包」这类最隐蔽的部署失误
- 待办：Task 8 步骤 5 —— **需用户手工验收**（新建聚合供应商 → 启用 → 重启 Claude Desktop →
  在选择器里逐个选不同家的模型 → 查日志确认打到不同上游）
- 验收提醒：① 代理面板 active_targets 显示目标供应商属正常（不是当前供应商被切走）；
  ② 探针 PROBE自定义ID 会随本次 profile 重写自然消失

## 事故：单元测试写坏了真实 Claude Desktop profile（2026-09-22 14:13 发现 / 15:0x 修复）

**现象**：用户验收时 CC Switch 顶部报「Claude Desktop profile 指向的地址与当前供应商不一致；
当前为 https://aggregate.example，应为 http://127.0.0.1:15721/claude-desktop」。

**取证（不是推测）**：
- 真实 profile 与 10:28 的已知良好备份逐键 diff：**只有 3 处差异**
  · `inferenceGatewayBaseUrl`: 应为 `http://127.0.0.1:15721/claude-desktop` → 实为 `https://aggregate.example`
  · `inferenceGatewayApiKey`: 应为 `ccs-b16dfe…1d0b`（DB 中的真 token，未变） → 实为 `agg-token`
  · `inferenceModels`: 4 项 → **整个键消失**
  · **其余 19 个共有键逐字节相同**（含 `deploymentDisplayName: "Chris"`）→ 补丁 A 的合并语义工作正常，
    被覆盖的只有网关三键
- `aggregate.example` / `agg-token` 这两个串在**整个仓库里只出现一处**：
  `services/provider/mod.rs` 的测试夹具 `claude_desktop_direct_settings()`
- DB 里 0 次出现（`grep -a` 全库）→ 不可能是应用或用户写进去的
- 无 `inferenceModels` + Direct 语义 ⇒ 写入者是一个**直连模式**的供应商，其 env 恰为夹具值

**根因**：`services/provider/mod.rs` 新加的聚合测试用了 **`with_test_home`**，
而该 helper 只隔离 `CC_SWITCH_TEST_HOME` / `HOME` —— **没有隔离 `LOCALAPPDATA`**。
Windows 上 `claude_desktop_config::windows_local_app_data_dir()` 直接读 `LOCALAPPDATA`，
于是 `ProviderService::add(state, AppType::ClaudeDesktop, …)` 在「无当前供应商」时会
（`mod.rs` 的 `add` 分支）把该供应商设为当前并 `write_live_with_common_config_for_state`
→ `apply_provider` → **写进开发者真实的 3P profile**。

同模块上方其实有正确的 helper `TempHome`（设 `HOME`/`LOCALAPPDATA`/`USERPROFILE`/`CC_SWITCH_TEST_HOME`），
既有的 Claude Desktop 测试（如 `update_current_claude_desktop_provider_…`）用的就是
`TempHome::new()` + `#[serial]`——是我选错了 helper。

**修复**：
1. `with_test_home` 补上 `LOCALAPPDATA` 隔离（设成 `<temp>/AppData/Local` 并在结束时还原）——
   一处改动关闭该 helper 全部 46 个调用点的同类隐患
2. 新增的 4 个触及 Claude Desktop 的测试加 `#[serial]`——
   与 `TempHome` 系（`#[serial]`）互斥，消除「别的 helper drop 时把 LOCALAPPDATA 还原成真值」的交叉竞态
3. 生产代码未改：`windows_local_app_data_dir()` 保持只读 `LOCALAPPDATA`

**恢复**：把 10:28 的真值备份写回 profile（22 键、正确网关地址与 token、4 个模型），
被写坏的版本另存为 `clobbered-by-test-20260922.json` 备查。

**遗留（建议单独立项）**：`with_test_home`（持模块内 `test_guard`）与 `TempHome`（用 `#[serial]`）
用的是**互不相干的两把锁** → 两族测试仍可并发互换 `LOCALAPPDATA`。本次已把自己的测试
用 `#[serial]` 摘干净；其余 `with_test_home` 测试目前不写 Claude Desktop 路径，
但该组合仍是一颗雷（彻底解法：两族共用同一把锁）。

**验证（同上，修复后全量 `cargo test --lib`）**
- **真实 profile md5 前后完全一致**：`38ffcf39a045f6cde8bd73b83f57f763`，mtime 也没变（14:57:02）
  → 全量测试不再碰真机文件，隔离修复成立
- 结果 2852 passed / 12 failed / 6 ignored；与修复前逐条对齐：
  · **`validate_aggregate_rejects_ordinary_provider_becoming_referenced_aggregate` 不再失败**
    （原先在并行全量下被 TempHome 竞态带崩，`#[serial]` 一并治好了这个 flaky）
  · 余下 10 项 = 修复前那 11 项去掉上面这一条（model_pricing × 5、import_hermes、
    update_current_claude_desktop（端口 10048 被运行中的 CC Switch 占用）、
    session_usage_grokbuild、codex_config、commands::misc）
  · 多出 2 项 `coding_plan::tests::transient_*` —— 隔离单跑**全过**（3 passed），
    是并行全量下的回环端口抖动，与本次改动无关

## 增强：聚合槽位支持「获取模型列表」（2026-09-22 16:0x）

**触发**：用户验收时反馈「不能获取模型，必须手动输入模型」。核对属实——原实现里
上游模型只有裸 `Input`，而既有的「模型映射」行是 `Input` + `ModelDropdown` +
右上角「获取模型列表」按钮（`handleFetchModels` → `fetchModelsForConfig`）。用户明确
要求采用与既有 UI 一致的按钮形态。

**为什么不能复用顶部那个按钮**：该按钮用的是**当前供应商自身**的 baseUrl/apiKey，
而聚合供应商按设计无端点无凭据；槽位要拉的是**目标供应商**的列表。且 `needsModelMapping
&& !isAggregate` 已把聚合的模型映射段整个隐藏，所以聚合这边必须有自己的入口。

**改动（两个前端文件，后端未动）**：
- `ClaudeDesktopProviderForm.tsx`：新增 `aggregateModelsByProvider`（按**供应商 id** 缓存，
  同家的多个槽位共用一份）+ `fetchingAggregateProviderId`，以及
  `handleFetchModelsForProvider(provider)`——用既有的 `envString` 读该供应商的
  `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN`，复用 `fetchModelsForConfig` 与
  `showFetchModelsError`、成功 toast 文案
- `AggregateProviderFields.tsx`：上游模型标签行右侧加「获取模型列表」按钮（loading/禁用
  与既有按钮同款），输入框右侧条件渲染 `ModelDropdown`
- 按**供应商 id** 而非槽位索引缓存：改档位会让 routeId 重编号、删除槽位会让索引位移，
  两者都会让缓存错位；挂在供应商上则天然稳定且可共享
- 输入框保持可手填（中转站的 /v1/models 未必列全）

**未做（用户未答，保持最小改动）**：选中模型后自动填「显示名」。既有模型映射行的行为是
`labelOverride || id`，若要对齐一行即可加上；当前保持不自动填。

**验证**：`pnpm typecheck` 通过；`npx vitest run` 1120 passed / 3 failed（全部是
`PiProviderForm.test.tsx` 的 5s 超时，单跑该文件 46/46 全过，并行负载抖动，与本次无关）。

## 事故二：槽位 ID 把供应商名 slug 进去 → Claude Desktop 删光整组模型（2026-09-22 21:25 实测 / 22:0x 定位）

**用户报告**：「不行啊」+ 两张截图：① CC Switch 弹红字 `Test 检查出错: Failed to extract
base_url: 配置错误: Claude Provider 缺少 base_url 配置`；② Claude Desktop 的 3P 面板底部
`⚠ Invalid: Model list`。

### 症状 A：聚合供应商点「检测连通」报 base_url 缺失

- **根因（已复现）**：`services/stream_check.rs::resolve_base_url` 只对 `category == "official"`
  短路，聚合供应商（按设计 `settings_config.env` 为空）会落到 `ClaudeAdapter::extract_base_url`，
  抛出误导性的「缺少 base_url 配置」。TDD 红灯证据（断言原文对比）：
  ```
  left:  "Failed to extract base_url: 配置错误: Claude Provider 缺少 base_url 配置"
  right: "Aggregate providers do not expose a reachability-check target"
  ```
- **参照实现**：官方供应商同一 guard 已存在（同函数首段）；`ProviderCard.tsx` 的 `onTest`
  本就对官方隐藏按钮、注释写明「没有可靠的探测目标」，聚合属同一类。
- **修复**：`resolve_base_url` 增加聚合短路；`ProviderCard` 的 `onTest` 门控同步排除聚合。

### 症状 B：模型列表被 Claude Desktop 整组删除（真正的「不行」）

**证据链（不是推测）**——Claude Desktop 自己的日志
`%LOCALAPPDATA%\Claude-3p\logs\main.log`：

```
21:25:23 [warn] inferenceModels: "claude-fable-deepseek" is not an Anthropic model and was removed from the list
21:25:23 [warn] inferenceModels: "claude-fable-zhipu-glm" is not an Anthropic model and was removed from the list
21:25:23 [warn] inferenceModels: "claude-opus-deepseek" is not an Anthropic model and was removed from the list
21:25:23 [warn] inferenceModels: "claude-opus-zhipu-glm" is not an Anthropic model and was removed from the list
21:25:24 [info] [custom-3p] Model discovery: skipped (inferenceModels obviates discovery); picker = 0 (empty)
```

那 4 个串正是「Test」聚合供应商的槽位 routeId（DB 已核对）。**用户确实启用了它，而
Claude Desktop 把四个槽位全删了，选择器变空。**

**校验器（从 app.asar 反解，逐字确认）**：

```js
Ho  = ["sonnet","opus","haiku","fable","mythos"]           // 角色白名单
Wo  = new RegExp(`^(${Ho.join("|")})(-[\d.]+)?$`)          // 裸档位别名
Bxe = ["claude", ...Ho, "anthropic"]
Vxe = /ark-code|astron|…|deepseek|doubao|gemini|gemma|glm|gpt|grok|…|kimi|…|qwen|…/  // 厂商词
Go  = (name) => Vxe.test(name.toLowerCase()) ? false : (Wo.test(name) || Bxe.some(x => name.includes(x)))
Yxe = (name) => Go(name) ? {ok:true}
             : {ok:false, reason:"expected a gateway model route referencing an Anthropic model (…)"}
```

即 **厂商词优先否决**：名字里只要含 deepseek/glm/kimi/gpt… 就整条作废（`/claude-fable-deepseek`）。

**根因（我的设计缺陷）**：槽位 ID 方案 `claude-{档位}-{供应商名 slug}` 把供应商名写进了
模型 ID，而该用户的供应商正好叫 **DeepSeek** 与 **Zhipu GLM**——两个词都在黑名单里，
四条槽位全灭。后端 `is_claude_safe_model_id` 当时只查角色前缀（`fable-deepseek` 的 tail
以 `fable-` 开头、且其后非空 → 放行），所以**一路绿灯写进 profile，再被对面悄悄删光**。

**修复**：
1. `utils/aggregateRoutes.ts`：ID 改为 `claude-{档位}-{同档序号}`（`slotId`），
   供应商名不再进入 ID；可读性交给「显示名」（`labelOverride`，不受该校验约束）。
   随之删除已无调用方的 `slugify`/`generateSlotId`。`assignSlotIds` 改为
   按档位编号 + 让引用槽位 ID 的默认目标**按位置**跟随（目标槽位被删则保持原值）。
2. `ClaudeDesktopProviderForm`：**载入即迁移**存量 ID（纯重编号，未保存不落库），
   否则用户手里那份带坏 ID 的供应商会继续写坏 profile。
3. `claude_desktop_config.rs::is_claude_safe_model_id`：补上厂商词黑名单（`Vxe` 逐字
   转写，用 `regex` 依赖，词边界保留），使后端校验与 Claude Desktop 的**实际规则**一致。
   否则存量坏 ID 会被静默写进 profile（`aggregate_model_routes` 只丢自己认得的坏 ID），
   再被对面删光——本次就是这种「静默失败」让人白白排查了很久。
4. 连带把 5 个 Rust 文件里 28 处 `claude-sonnet-glm` / `claude-haiku-ds` 类夹具改名为
   新方案（`claude-sonnet-1` / `claude-haiku-1`）——它们编码的正是被证伪的旧方案。

**未改动（刻意）**：`model_list_response` 无需改——它走 `proxy_model_routes`，已对聚合短路；
探针实测 `GET /claude-desktop/v1/models` 返回 200 且 id 与 profile 的 `inferenceModels`
逐字一致（所以症状 B 不是 discovery 不匹配）。

**验证（2026-09-22 22:1x）**
- 前端：`npx vitest run` 全量 1116 passed / 4 failed，失败项全是并行负载下的
  5s/10s 超时（PiProviderForm ×3、App.test ×2、SettingsDialog ×1，且每次跑名单还会变）；
  单跑这 3 个文件 58/58 全过 → 判定为抖动，非回归。逐次跑的数字：3 → 4 → 6，
  基线（改动前）也是 3 —— 数量本身不稳定，所以必须看名单而不是看数字。
  `pnpm typecheck` 通过。
- 后端：`cargo test --release --lib` **2856 passed / 10 failed**，10 项即既有环境性失败
  （model_pricing ×5 读真实用户配置、import_hermes、update_current_claude_desktop
  （端口 10048 被运行中的 CC Switch 占用）、session_usage_grokbuild / codex_config
  （符号链接权限 1314）、commands::misc）。
- **中途踩了自己的坑**：批替换夹具时漏掉变体 `claude-opus-glm`，导致
  `validate_aggregate_accepts_valid_table` 变红——恰好是新厂商词规则正确拒绝的结果。
  改用脚本把**全部** Rust 源码里的 `claude-*` 字面量过一遍黑名单，确认余下 10 处
  都是应当被拒绝的（测试反例 + 一条与 profile 无关的 usage_stats 定价用例），
  避免"手改一处漏一处"。
- 副作用（可接受）：报错文案「槽位 ID 不合法（须形如 …）」里的示例被批替换顺带改成
  `claude-sonnet-1`，正好与新方案一致。
- 未被单测覆盖：表单「载入即迁移」的那 3 行接线（迁移逻辑本身有测试）。留待本机验收。

**构建与部署（2026-09-22 22:46 构建 / 23:06 上线）**
- 构建 15m44s → `CC Switch_3.20.3-local_x64-setup.exe`
- 产物 `cc-switch.exe` md5 `d9d43a8f92e88b835bd2d5d8125d4883`、34438656 字节、
  内嵌前端资源 `index-DW7iqeCK.js`（与 dist/index.html 引用一致）
- 部署：按 UPSTREAM-SYNC §6 的新口径——**先轮询确认进程退出**（1s 内退出）再装，
  未再复现上次的静默失败；装入后 md5 一致、pubkey = 0
- 运行：pid 12400，ProductVersion 3.20.3-local
- 交接：用户需打开「Test」→ **保存一次**（载入即迁移槽位 ID）→ 启用 → 重启 Claude Desktop
- 待用户确认的选项：槽位「显示名」是否默认填「供应商名 · 上游模型」
  （ID 不再含供应商名后，显示名成了唯一的可读标识）

## 首次端到端自验（2026-09-22 23:2x–23:5x，由控制器亲自做，未假手用户）

起因：用户要求「你自己先添加供应商验证」。做法与结论：

**入口调研**：deeplink（`ccswitch://v1/import`）**不能**用于此——`parse_provider_deeplink`
的 app 白名单是 claude|codex|gemini|grokbuild|opencode|openclaw|hermes，**没有
claude-desktop**；且它只能带 `endpoint/apiKey/usageScript` 等字段，**带不了
`meta.aggregateRoutes`**。proxy 侧也只有数据面（/health、/status、/v1/models、
/v1/messages…），**没有切换供应商的控制面**。故只能走 DB。

**做法（全程可回滚，DB 与 profile 均已备份到 `%TEMP%\ccsw-verify-<ts>\`）**
1. 停 CC Switch → 把「Test」的 4 个槽位 ID 按新方案重写（复刻 `assignSlotIds`
   的档位序号 + 默认目标按位置跟随）→ 写回 DB
2. **踩到「当前供应商」有两处存储**：真正的权威是
   `~/.cc-switch/settings.json` 的 `currentProviderClaudeDesktop`；
   DB 的 `providers.is_current` 只是 fallback（见 `settings::get_effective_current_provider`）。
   只改 DB 不生效——这正是第一次试请求仍落到 DeepSeek 的原因。
3. 改 settings.json 后重启 → 聚合供应商成为当前供应商

**验证结果（全部来自 app 自身的接口与日志，非推测）**
- `GET /claude-desktop/v1/models` → `[claude-fable-1, claude-fable-2, claude-opus-1, claude-opus-2]`
- 四个槽位各发一条真实请求，`proxy_request_logs` 归属：
  · claude-fable-1 → glm-5.3        → **Zhipu GLM**
  · claude-fable-2 → deepseek-v4-pro → **DeepSeek**
  · claude-opus-1  → glm-5.3-flash  → **Zhipu GLM**
  · claude-opus-2  → deepseek-flash → **DeepSeek**（四条全 200）
  → 一供应商内按模型分流到两家不同上游，且模型名改写正确 ✓
- profile 中的模型 ID 已改为那四个（由控制器按 app 的 `inference_model_json` 语义写入）

**新发现（by design，但值得记住）**：CC Switch **只在显式切换/更新供应商时**写 profile，
**启动时不写**（两次重启 + md5/mtime 实测确认：DB 与 settings 都指向聚合供应商时，
profile 依旧纹丝不动）。故「改完配置没重启 Claude Desktop 却发现 profile 没变」不是 bug。
副作用：那条「addr 不一致」横幅的修复建议（重新切换一次）确实是唯一路径。

**未能自验的一步**：Claude Desktop 的判词（`picker = N` / 有无 "is not an Anthropic model"）
只在它启动时写入日志；而重启它会连带结束本会话（Code 面板由该 app 承载），故留给用户。
佐证：Claude Desktop **不监听** profile 文件——写入后 15s 内日志零新增。

## 2026-09-23 用户侧验收通过（终局）

- 用户重启 Claude Desktop 后截图：模型选择器出现 **Claude Fable 1/2、Claude Opus 1/2**
  共 4 模型 × 各带 1M 变体共 8 行，档位描述（toughest challenges / ambitious work）
  被 app 按 tier 正确识别，无 "Invalid: Model list"。
- main.log 判词：`picker = 4 (inferenceModels)`（09-22 21:33 与 09-23 00:01 两次）；
  厂商词拒收警告仅存在于修复前（21:25），之后零新增。
- 用户实际切到 `claude-opus-1[1m]` 使用——聚合路由在真实会话中服役。
- 收尾：默认目标布局修复已提交（`d60cc905`，与已部署二进制一致）。

槽位显示名仍是默认派生（"Claude Fable 1" 等）；如需自定义可在槽位编辑器填「显示名」。
