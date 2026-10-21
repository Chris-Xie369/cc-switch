# 上游同步与本地维护

本工作区维护一个本地构建的 CC Switch（修复 3P profile 覆盖缺陷 + 显示名可配置），同时跟踪上游 `farion1231/cc-switch`。

**当前基线**：tag `v4.0.17`（提交 `19798e2f`，2026-10-21 同步）。上一基线是 `v4.0.16`（`f92ff0b0`）。

---

## 1. 仓库布局

| 名称       | 指向                                            | 说明                                                                                       |
| ---------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `upstream` | `https://github.com/farion1231/cc-switch.git`   | 官方仓库（存储规范地址；全局 `insteadOf` 会把 github.com 改写成 gh-proxy，拉取自动走代理） |
| `origin`   | `https://github.com/Chris-Xie369/cc-switch.git` | **你的 fork**，推自己的改动用。push 走独立 pushurl，见下                                   |
| 分支       | `fix/profile-merge`                             | 工作分支，已推送到 fork；**绝不推送到 upstream**                                           |

工作树在 `src/`（本目录下）。改动提交都落在这个分支上。

### 推送通道（重要，2026-09-21 实测）

**gh-proxy 只代理下载，不支持 push**（`info/refs?service=git-receive-pack` 返回 405）。而全局 `insteadOf` 会把所有 `https://github.com/` 改写成 gh-proxy —— 因此直接 `git push` 必然失败（且会弹用户名提示）。

**解法**（已配置在 origin 上）：给 fork remote 单设 pushurl，用**带用户名的 URL 形式**绕开前缀匹配，让它直连 github.com（仍经本机 9674 代理）：

```bash
git remote set-url --push origin "https://Chris-Xie369@github.com/Chris-Xie369/cc-switch.git"
git push origin fix/profile-merge
```

原理：`https://用户名@github.com/…` 不以 `https://github.com/` 开头 → 不被 `insteadOf` 命中；主机仍是 `github.com` → 现成的 gh 凭据助手（`gh auth git-credential`）能正确提供 token。

> 注意：直连 github.com 需要走 9674 代理（仓库里配了 `https.proxy`）；若换了网络环境，先确认 `curl -sI https://github.com` 有响应。

---

## 2. 我们携带的补丁

用 **merge** 方式同步（见 §3）。下面是同步时必须保住的代码。历史提交还在 git 里，但不要再把已退役的补丁当冲突时的「我们这边」。

| 补丁               | 内容                                                                                                                                                                                                                         | 上游化状态                                                           |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| **B** 显示名可配置 | `ClaudeDesktopDisplaySettings`（`settings.rs` + `claude_desktop_config.rs` 的 `inject_display_settings`）+ GeneralSection + i18n。`endUserAttribution=false` 属于这一补丁：关掉后 Desktop 左下角只留显示名，不渲染 OS 用户名 | 上游无此功能（PR #7779）；值得继续推                                 |
| **C** 本地构建     | 移除 `plugins.updater` / `createUpdaterArtifacts`；版本号 `<版本>-local`                                                                                                                                                     | **纯本地**，永不上游                                                 |
| **D** 上游状态检查 | 「关于」页检查 fork 是否落后、是否有新 release（`commands/upstream.rs` + `lib/updater.ts` + `UpdateContext` + `AboutSection`）                                                                                               | **纯本地**                                                           |
| **DSH**            | DeepSeek Harness 供应商与 `cordis.patch.yml` 里的 `cc-switch` 路由（`dsh_config.rs`、`commands/dsh.rs`、`DshProviderForm`、`dshProviderPresets`）                                                                            | 本地功能。已有的 `cc-switch` 路由在 Desktop Stack 投影后刷新模型列表 |
| **x-api-key**      | `gateway_token_from_headers` 同时认 `Authorization` 与 `x-api-key`                                                                                                                                                           | DSH 需要。上游候选                                                   |
| **厂商词守卫**     | `has_non_anthropic_vendor_token`：模型 id 含 deepseek / glm / kimi / gpt 等词时拒绝写入 Desktop profile                                                                                                                      | 上游候选。一个坏 id 会让 Desktop 整组模型消失                        |

**已退役，同步时不要捡回来：**

- **A** profile 读-合并-写。上游 `81df5a08` 的 `gateway_profile_patch` 只覆盖 `DESKTOP_PROFILE_FLOOR` 网关键，其余字段原位保留。PR #5417 只作历史对照。回归测试 `claude_desktop_apply_preserves_non_gateway_profile_fields` 仍要过。
- **E** 聚合供应商。2026-10-20 物理删除。多家模型同时出现在 Desktop 选择器里，用上游 Stack（`DesktopStackModel` / `route_profile`）。不要恢复 `aggregate.rs`、聚合路由表、聚合编辑器、别名路由、1M 聚合路由、CLI `modelPicker` 镜像。
- **F** settings.json 合并。上游关键字段写入已经覆盖。`services/config.rs` 与 Claude 的 live 写路径相对 `v4.0.16` 没有本叉专有的合并差异。
- 「diagnostics Gateway unreachable」是当时的配置改动，不是要携带的代码。

**DSH 的触及面（同步时注意）**：`dsh_config.rs`、`commands/dsh.rs`、`mode/controller.rs` 里 Desktop 投影之后的 `sync_dsh_aggregate_now`、`DshProviderForm.tsx`、`dshProviderPresets.ts`。模型 id 和显示名来自 `desktop_published`，不是旧的聚合槽位。

> 还有若干 `docs(...)` 提交（设计/计划/账本），是本地决策记录，与上游无关。

**核心原则：携带面越小，维护越省。** B、x-api-key、厂商词守卫推上去之后，本地长期只剩 C、D 和 DSH。

---

## 3. 同步流程（merge，不要 rebase）

```bash
cd "D:/Workspace/Project/cc-switch/src"
git fetch upstream --tags --prune
git tag --sort=-v:refname | head -3        # 看有没有新版本
git merge v3.21.0                          # 或 git merge upstream/main（跟未发布改动）
# ...处理冲突、改版本号、重新构建并验收之后：
git push origin fix/profile-merge          # 把同步结果备份到 fork
```

用 merge 而非 rebase 的理由：rebase 每次都要重放你所有自有提交，同样的冲突每次重演；merge 只解决一次，之后 merge base 前移，git 记住了。

**上游有新 release 时**（推荐跟 tag，最稳）：对照 §2。A、E、F 都已不在携带面里。官方版仍然替代不了本地构建，因为 B（显示名）、C、D、DSH，以及 x-api-key 和厂商词守卫还在。

---

## 4. 冲突热点（实测数据，2026-09 上游改动频次）

| 文件                                     | 上游 9 月改动次数 | 冲突原因                                 | 处理                                                                                                                                                                      |
| ---------------------------------------- | ----------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/i18n/locales/*.json`                | 各 11 次          | 补丁 B 也改它                            | 我们只加 `settings` 下 6 个连续键；取上游版本后重加这 6 个即可                                                                                                            |
| `src-tauri/tauri.conf.json`              | —                 | 补丁 C 每次必冲突（版本号 + 删 updater） | 机械处理：版本号改成新 tag 的 `<版本>-local`，再删一次 updater 三处                                                                                                       |
| `src-tauri/src/claude_desktop_config.rs` | —                 | 补丁 B 与厂商词守卫                      | 保住 `inject_display_settings`（含 `endUserAttribution`）和 `has_non_anthropic_vendor_token`。profile 非网关键由上游 `gateway_profile_patch` 保留，不要再加一套读-合并-写 |
| `README.md`                              | 上游高频          | 2026-09-29 我们自建 fork 门面 README     | 上游改 README 时取上游版，再重放我们的门面内容（内容多为自有，冲突机械解）                                                                                                |

---

## 5. 构建

```bash
cd "D:/Workspace/Project/cc-switch/src"
export PATH="/c/Users/Jason/.cargo/bin:$PATH"
export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR="https://gh-proxy.com/https://github.com/"
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy

pnpm install                 # 依赖有变时
cargo test                   # 在 src-tauri/ 下；打补丁后先测
pnpm tauri build --bundles nsis
```

- 镜像与绕过代理是**必需**的：tauri CLI 走 9674 代理下载 NSIS 工具会被 gh-proxy 拒 400
- 版本号三处必须一致：`tauri.conf.json` + `Cargo.toml` + `Cargo.lock`（`cargo check` 可同步 lock）
- 产物：`src-tauri/target/release/bundle/nsis/CC Switch_<版本>-local_x64-setup.exe`

**磁盘**：`cargo test` 走 debug profile，会生成 `src-tauri/target/debug/`（15–24 G，主要是各依赖的 `.pdb` 调试符号与增量缓存）。两种处理：

```bash
# 方案一：测试完清掉 debug 树（保留 release，重建安装包仍然快）
MSYS_NO_PATHCONV=1 cmd /c "rmdir /s /q D:\Workspace\Project\cc-switch\src\src-tauri\target\debug"
```

```bash
# 方案二：直接用 release profile 跑测试，压根不生成 debug 树（编译稍慢、失败时的回溯信息略少）
cd "D:/Workspace/Project/cc-switch/src/src-tauri" && cargo test --release
```

---

## 6. 部署

```bash
MSYS_NO_PATHCONV=1 taskkill /F /IM cc-switch.exe
```

```bash
MSYS_NO_PATHCONV=1 cmd /c "D:\Workspace\Project\cc-switch\tools\install-local.bat"
```

```bash
cmd //c start "" "C:\Users\Jason\AppData\Local\Programs\CC Switch\cc-switch.exe"
```

- 必须经 `.bat` 调用安装器：Git Bash 会转写 `/S` 与 `/D=`，导致静默装到 NSIS 默认目录（踩过一次）
- **装之前必须确认进程真的退出了**（`taskkill` 之后轮询 `tasklist`，别只 `sleep`）：exe 被占用时
  NSIS 静默安装会**静默失败**——`EXITCODE` 仍是 0、目标文件纹丝不动，还顺手把你原来那个旧版本
  重新拉起来，看上去像"装成功了"（2026-09-22 踩过一次，白折腾一轮）
- 退出 cc-switch 会中断经其网关的会话；它是独立进程，稍后重启即可恢复
- 校验装对了没（**不要看版本号**，Windows 会丢 semver 预发布标签）：
  - `md5sum` 与 `target/release/cc-switch.exe` 比对（比 `stat -c%s` 更强：同尺寸不同内容也能分辨）
  - 二进制中不应含官方 pubkey：`grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6"` = 0
  - **确认前端也换了**：`grep -a -c "<dist/index.html 里那个 assets/index-*.js 文件名>" <已安装 exe>` 应为 1
    （前端资源在 release 里是压缩的，中文 UI 文案 grep 不到——只有资源名这类明文串能查，
    这是唯一能排除"后端是新的、前端还是旧的"的廉价手段）

---

## 7. 同步后必须重新验收

上游可能改动了 profile 写入路径，**不能假定上次的修复仍有效**。至少跑一次：切换供应商 → 检查 profile 键数没骤降、非网关字段（含你自己的面板设置）全部存活。口径见 CLAUDE.md「验证口径」。

---

## 8. 安全更新

CC Switch 持有全部供应商 API 密钥并代理全部流量。分叉落后于上游 = 错过上游的安全修复。**建议即使没有想要的新功能，也按月同步一次**；commit 信息含 security / 密钥 / 代理相关字样的优先跟。

---

## 9. 降低长期成本的路径

- **补丁 A** 已由上游 `81df5a08` 取代。PR #5417 只留作历史对照，不要再把那套读-合并-写捡回来。
- **补丁 B**（显示名，含 `endUserAttribution=false`）对上游其他用户同样有价值，继续推 PR #7779。
- **x-api-key** 与**厂商词守卫**是小块上游候选。DSH 依赖前者。
- B 与这两处都上游化之后，本地长期携带面是 C、D 和 DSH。
