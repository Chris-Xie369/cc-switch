# CC Switch（本地维护 fork）

这是 `farion1231/cc-switch` 的本地 fork，在官方版之上携带两类自有改动：

1. **Claude Desktop 3P profile 合并语义修复**——官方版每次应用供应商时整份覆盖
   profile（丢弃不认识的字段），本 fork 改为「读旧值 → 叠自有键 → 写回」（对应
   上游 PR farion1231/cc-switch#5417，长期未合并故本地自维护）。
2. **聚合路由（MoA）**——把多家供应商的多个模型映射进 Claude Desktop 的
   fable/opus/sonnet/haiku 四档，一次重启后选择器内自由切换；编辑器带
   思考能力三态徽标与 maxEffort 上限（本 fork 专属功能）。

> **仅限本机/知情者自用**：自构建版本会替换官方签名二进制，而 CC Switch 持有
> 全部供应商密钥并代理其流量。不要分发给非知情者。

## 分支与远端

- 工作分支 `fix/profile-merge`（基线 tag `v3.20.4`），推送到 `origin`（Chris-Xie369/cc-switch）
- `upstream` = 官方仓库，**只拉不推**
- 同步流程、补丁清单、冲突热点见 [UPSTREAM-SYNC.md](docs/local-maintenance/UPSTREAM-SYNC.md)

## Windows 构建与部署

工具链：Rust 1.95+（cargo 需手动进 PATH）、node 24+、pnpm 10+。

```bash
cd <repo>
export PATH="/c/Users/Jason/.cargo/bin:$PATH"
export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR="https://gh-proxy.com/https://github.com/"
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy
pnpm install          # 依赖有变时
pnpm tauri build --bundles nsis
```

- 镜像与绕代理是**必需**的：tauri CLI 经 9674 代理下载 NSIS 工具会被 gh-proxy 拒 400
- 产物：`src-tauri/target/release/bundle/nsis/CC Switch_<版本>-local_x64-setup.exe`
- 部署经 `docs/local-maintenance/install-local.bat`（原件在工作区 `tools/`，不在本仓库根）（Git Bash 直跑安装器会转写 `/S` `/D=` 参数）；
  装前必须轮询确认旧进程退出（exe 被占用时 NSIS 静默失败且 EXITCODE=0）
- 装后校验（**不看版本号**）：`md5sum` 对比 `target/release/cc-switch.exe`；
  `grep -a -c "<dist/assets/index-*.js 名>" <安装 exe>` 应为 1（确认前端也换了）；
  `grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6"` 应为 0（不含官方签名公钥）

## 已知耦合：Claude Desktop 黑盒知识（升级后须重验）

聚合槽位的**思考能力**由 Claude Desktop 对模型 ID 的判定决定（逆向自 2.9939.4.0）：

| 能力 | ID | 表现 |
|---|---|---|
| 完整强度阶梯（含 xhigh） | `claude-sonnet-5`、`claude-opus-4-7/4-8`、fable/mythos 族 | low…max 滑条 |
| 完整强度阶梯（无 xhigh） | `claude-sonnet-4-6`、`claude-opus-4-6` | low/medium/high/max |
| 完整强度阶梯（池不收，强制思考） | `claude-opus-5`（`disallowThinkingDisabled`） | low…max 滑条 |
| 仅扩展思考开关 | `claude-sonnet-4-5`、`claude-haiku-4-5` | 只有开/关 |
| 无 | 其余全部（自造溢出 ID） | 无任何控件 |

- ID 另受「厂商词黑名单 + Anthropic 形状」校验：名字含 deepseek/glm/kimi/gpt 等
  会被**整组**从模型列表剔除——槽位 ID 恒为 `claude-{tier}-*` 形状，与供应商名无关
- 判定表镜像在 `src/utils/claudeDesktopCapability.ts`（含重验 grep 手册）；
  Desktop 升级后跑一次手册、按需更新表
- `supports1m` 是对 Desktop 的**能力声明**（选择器出 `[1m]` 变体并按 1M 管理上下文），
  上游是否真支持须逐模型实证；代理转发上游时剥掉 `[1m]` 标记

## 已知供应商怪癖（按家硬编码在 forwarder）

- **OpenCode Go**（`opencode.ai/zen/go`）：要求 `x-opencode-session` 头 + 浏览器
  User-Agent（缺一被 Cloudflare/网关拒），已在前向器注入；其他供应商若遇类似
  门槛，当前需要改代码（未做 per-provider 配置化——YAGNI）

## 验收口径

改配置/升级后至少跑一轮：代理 `127.0.0.1:15721/claude-desktop` 的 `/v1/models`
列出全部槽位 → 每槽一条真实请求全 200 → 切换供应商后 profile 键数不骤降、
非网关字段（面板设置等）全部存活。
