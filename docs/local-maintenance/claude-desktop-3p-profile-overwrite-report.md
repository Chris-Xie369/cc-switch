# Claude Desktop 3P Profile 覆盖回退问题报告

- **日期**：2026-09-20
- **机器**：desktop-hd0n7rs（Windows 11 Enterprise，本地账户 `Jason`）
- **涉及组件**：Claude Desktop 2.2553.1（3P 模式）、CC Switch 3.20.3（MIT 开源，farion1231/cc-switch）、claude3p-profile-guard（守护进程，本仓库 Tmp 下）

---

## 1. 现象

1. Claude Desktop 左下角用户标签显示 **"Jason / Gateway"**（用户菜单同），用户期望显示 **"Chris / Gateway"**。
2. 通过写入 3 个配置键并重启应用后，9/18 确认生效为 "Chris / Gateway"。
3. 9/20 早上发现**又变回 "Jason / Gateway"**（模型标签同时变化，因为供应商被切换过）。
4. 规律：**每次 CC Switch 应用/切换供应商后，profile 中所有非 CC Switch 模板字段都被抹掉**。自 7/17 以来 CC Switch 共写入 profile 82 次，每次都与切换/后台 reconcile/代理启动事件一一对应（9/18 13:58 至 9/20 08:28 之间零写入——写入完全由事件驱动，无定时器、无文件监听器）。

---

## 2. 根因（三层，均已在源码/二进制/日志层面验证）

### 2.1 第一行 "Jason" 为什么改不了 —— Claude 官方设计（静态限制）

- 3P 模式身份行解析链（app.asar 主进程代码）：凭据源 `principalIdentity()` → `hybridAccountIdentity()`（仅 bootstrap 场景）→ **`os.userInfo().username` 兜底**（`source: "os"`）。
- 当前凭据是 static bearer key（`ccs-b16dfe…`），无 id_token claims，静态凭据源的 `principalIdentity()` 是空函数 → 必然落到 OS 用户名兜底 → 显示 Windows 账户名 "Jason"。
- 无任何配置键可改身份行（全部 171 个 3P 配置键中不存在）；`PUT /api/account` 携带 `full_name`/`display_name` 被主进程明确返回 **403**。
- `endUserAttribution`（默认开启）是刻意设计：身份归属即"凭据身份，无声明时用 OS 登录名"，并伴随 `enduser.id` OTel 遥测属性。**此层只造成静态限制，不会导致任何配置"变回去"。**

### 2.2 改好的配置为什么必然消失 —— CC Switch 全量覆盖（动态事故的主体根源）

源码证据（GitHub main 分支，与安装版 3.20.3 一致）：`src-tauri/src/claude_desktop_config.rs`

- `apply_provider_to_paths_inner`（L974–1011）：每次应用供应商，先 `build_gateway_profile(...)` **从模板新造 JSON**，然后 `write_json_file(&paths.profile_path, &profile)` **整份覆盖**（L1007）——不读旧文件、不保留未知键。
- `build_gateway_profile`（L1026–1046）：写死的 7 键模板：

```json
{
  "coworkEgressAllowedHosts": ["*"],
  "disableDeploymentModeChooser": true,
  "inferenceGatewayApiKey": "<token>",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:15721/claude-desktop",
  "inferenceProvider": "gateway",
  "inferenceModels": [...]
}
```

- 行为证据：guard 捕获的 `last_external_write` 恰为这 7 键，与模板逐键逐值一致；CC Switch 二进制字符串中可见同一模板（紧邻 `src\claude_desktop_config.rs` 路径字面量）。
- **疏漏而非有意设计**：同一文件中对其他所有文件的写入都是"读旧值 → 改自己的键 → 写回"的合并模式（`write_deployment_mode` L1100、`write_meta` L1140、`remove_cc_switch_enterprise_config` L1118 均先 `read_json_or_empty`）；唯独 profile 走全量重建。作者对"保留未知字段"早有既有模式，只是没用到这一处。

### 2.3 为什么 9/20 没人救回 —— guard 自启静默失效（事故的直接扳机）

- guard 的 HKCU Run 自启项存在且未被禁用，但**从未成功自启过**：对比系统开机事件（9/18 06:25、9/19 12:45）与 guard 日志中的启动记录，两次开机均无 guard 启动痕迹；同批 Run 项中 CC Switch 均正常启动。
- `pythonw` 无控制台，启动期任何错误静默丢弃，无日志可查——启动方式本身的缺陷。
- 时间线：9/19 12:45 开机 → guard 失联 → 9/20 08:28:21 CC Switch 正常应用供应商、整份覆盖 profile → 08:30:06 Claude Desktop 启动，读到只剩 7 键的模板 → "Jason / Gateway" 回退。
- 对照：guard 存活期间（9/18 三次覆盖）100% 恢复成功（`apply(announced write): restored deploymentDisplayName,deploymentDisplaySubtitle,endUserAttribution,...`）——**机制可靠，失联才是问题**。

---

## 3. 解决方案

### 3.1 根治（推荐）：CC Switch 上游 —— profile 写入改合并语义

- 改动点：`apply_provider_to_paths_inner` 写入前先 `read_json_or_empty(&paths.profile_path)`，将 7 个自有键叠到旧对象上，再写回（CC Switch 拥有的键它赢，未知键保留——正是 guard overlay 模型搬进 CC Switch 内部）。约 10 行改动，完全遵循同文件既有模式（`write_deployment_mode` L1100、`write_meta` L1140 等）；MIT 协议。
- **上游已有同方向 PR：[farion1231/cc-switch#5417](https://github.com/farion1231/cc-switch/pull/5417)「fix(claude-desktop): preserve non-gateway profile fields on provider switch」**（提交者 huang-mian，2026-07-15 创建，截至 2026-09-20 仍为 open、未合并）。无需另起炉灶：可去该 PR 下支持/评论推动合并，或以该分支为基底本地应用。
- 合并发版后升级即可，**guard 可以卸载**，整条补丁链消失。

### 3.2 根治备选：本地 fork 自编译（不推荐，除非 PR 长期不合并）

- 本机有 node 24 / git，但**无 cargo**，需先装 Rust 工具链 + Tauri 依赖（首次构建较重）；
- 必须关闭 CC Switch 自动更新，否则官方版覆盖 fork；
- 以后每次想升级都要重新 merge + 编译——把"维护 guard"换成"维护二进制"。
- 自建路径的构建步骤与测试方案见并行产物 `Tmp/cc-switch-fix-brief.md`（与本报告互补，侧重建仓/编译/验证）。

### 3.3 根治备选：Claude 托管配置（Windows GPO 注册表策略）

- 托管配置是 Claude Desktop 的另一条配置来源（Windows 走注册表策略），CC Switch 够不到它——写入者唯一。
- 代价：应用进入"受组织管理"状态、部分设置变只读、改动触发重启卡片、需写 HKLM（管理员权限）。为一个显示名采用此方案属于过度设计，仅作为"绝对干净"的最后选项。

### 3.4 临时措施（当前生效状态）

- 三个键已恢复到 profile 文件（当前 19 键）：

```json
"deploymentDisplayName": "Chris",
"deploymentDisplaySubtitle": "Gateway",
"endUserAttribution": false
```

- guard 已于 2026-09-20 08:32 手动拉起（`--status` HEALTHY，overlay 14 字段）。
- **重启 Claude Desktop 后**左下角恢复显示 "Chris / Gateway"（配置在应用启动时读入）。
- 已知代价：Code 页顶栏问候语退化为不带名字的 "What's up next?"（问候语只读身份行，不读 deploymentDisplayName）；`enduser.id` 不再上报（未配置 OTel collector，影响仅此）。
- 用户已拒绝补丁式方案（计划任务看门狗未实施），下一步由 3.1 上游 PR 替代。

---

## 4. 关键证据与文件位置

| 项目 | 位置 |
|---|---|
| CC Switch 源码（分析用单份，未修改） | `Tmp/ccswitch_claude_desktop_config.rs` |
| 上游修复 PR（open，未合并） | [farion1231/cc-switch#5417](https://github.com/farion1231/cc-switch/pull/5417) |
| 并行技术简报（自建/构建步骤） | `Tmp/cc-switch-fix-brief.md` |
| 3P profile（当前 19 键） | `C:\Users\Jason\AppData\Local\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json` |
| profile 备份 | 同目录 `…json.bak-chrisgateway-20260918_*`、`…json.bak-namechange-20260918_*` |
| guard 状态（overlay 14 字段、last_external_write、templates） | `Tmp/claude3p-profile-guard/guard-state.json`（备份 `guard-state.json.bak-prename-*`） |
| guard 日志（9/18 三次成功恢复；9/19 开机后无记录） | `Tmp/claude3p-profile-guard/guard.log` |
| CC Switch 日志（82 次写入；9/20 08:28:21 最后一次） | `C:\Users\Jason\.cc-switch\logs\cc-switch.log` |

---

## 5. 一句话结论

- **静态限制**（"Jason" 不给改名）→ Claude 官方刻意设计；
- **动态事故**（配置改好又回退）→ CC Switch 的整份覆盖语义（必然性）+ guard 自启静默失效（本次扳机）；
- **根治路径** → CC Switch 上游合并语义 PR，合并后卸载 guard。
