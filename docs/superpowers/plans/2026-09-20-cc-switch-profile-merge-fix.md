# CC Switch 3P Profile 覆盖缺陷本地修复 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在本机编译出 CC Switch 3.20.3-local 安装包，修复 3P profile 覆盖缺陷（合并语义），并把左下角显示名纳入 CC Switch 自有字段（可配置），验收后删除 claude3p-profile-guard 守护进程。

**Architecture:** 在 tag `v3.20.3` 上应用两个补丁——补丁 A（上游 PR #5417 的合并语义，`claude_desktop_config.rs` 单文件）与补丁 B（新增 `ClaudeDesktopDisplaySettings` 设置 + profile 注入 + 前端设置项）。Rust 侧逻辑用单测覆盖，前端用 typecheck 把关；构建 NSIS 安装包覆盖安装到现有目录。

**Tech Stack:** Rust 1.95（Tauri v2 后端）、React + TypeScript（前端）、pnpm@10、NSIS（Tauri 打包）。

**设计文档:** `docs/superpowers/specs/2026-09-20-cc-switch-profile-merge-fix-design.md`（所有任务以此为唯一事实来源）。

## Global Constraints

- 版本号统一改为 `3.20.3-local`：`src-tauri/tauri.conf.json` 与 `src-tauri/Cargo.toml` 两处（tauri-build 校验二者一致）；`package.json` 的 version 不动。
- 分支 `fix/profile-merge` 仅本地，**绝不推送**；除下述列出的文件外不改动任何文件。
- 自动更新防覆盖 = **本地构建禁用 updater**（补丁 C，Task 5）：移除 `plugins.updater` 的 `pubkey`/`endpoints` 与 `bundle.createUpdaterArtifacts`，靠 `lib.rs` 既有的「配置不完整则跳过 updater 插件」逻辑生效。**不要**试图靠「本地签名密钥导致官方更新校验失败」来防覆盖——Tauri 的 pubkey 来自配置文件、构建时不会自动替换（已核实）；且 `3.20.3-local` 在 semver 中低于正式版 `3.20.3`，保留 updater 会主动提示升级到官方版。
- 本机路径（Windows / Git Bash）：仓库在 `D:\Workspace\Project\cc-switch\src\`；Rust crate 在 `src-tauri\`；前端在仓库根。`cargo` 命令在 `src-tauri\` 下执行，`pnpm` 命令在仓库根执行。
- 自己创建的提交（spec/plan/代码）结尾加 `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`；cherry-pick 的上游提交保留其原有署名，不改。
- 用户数据在 `%USERPROFILE%\.cc-switch\`（26MB SQLite DB 与供应商配置），安装器不动它，任何步骤都不得删除/迁移。

---

### Task 1: 安装工具链

**Files:** 无（系统级安装）

**Interfaces:**
- Produces: 可用的 `cargo`/`rustc`（Rust 1.95，MSVC 宿主）、`pnpm` 10.x、MSVC 链接器（`link.exe` + Windows SDK）、已装好的 `node_modules`。后续所有 cargo/pnpm 命令依赖于此。

- [ ] **Step 1: 安装 Rust（rustup）**

```bash
winget install --id Rustlang.Rustup -e
# 安装完成后新开终端，rustup 安装 rust-toolchain.toml 钉死的 1.95：
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
rustup toolchain install 1.95 --profile minimal --component rustfmt,clippy
rustup default 1.95
```

- [ ] **Step 2: 验证 Rust**

```bash
rustc -Vv   # 期望 host: x86_64-pc-windows-msvc；release 含 1.95
cargo --version
```

- [ ] **Step 3: 安装 MSVC 构建工具（C++ 工作负载 + Windows SDK）**

```bash
winget install --id Microsoft.VisualStudio.2022.BuildTools -e \
  --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --norestart"
```

- [ ] **Step 4: 安装 pnpm 10**

```bash
npm install -g pnpm@10.12.3
pnpm --version   # 期望 10.x
```

- [ ] **Step 5: 验证 MSVC 链接器可用（用一次性最小 crate 实测链接，而不是查 PATH）**

```bash
cd "$TEMP" && rm -rf ccs-linkprobe && cargo new ccs-linkprobe --bin && cd ccs-linkprobe && cargo build 2>&1 | tail -5
# 期望：Finished `dev` profile ...（链接成功，约 10 秒）
# 若报 "linker `link.exe` not found" → MSVC 未装好，回到 Step 3 重装并重启终端
```

- [ ] **Step 6: 安装前端依赖（Task 4 的 typecheck 与 Task 5 的构建都依赖它）**

```bash
cd "D:/Workspace/Project/cc-switch/src"
pnpm install   # 首次约数分钟
```

---

### Task 2: 补丁 A —— profile 写入改合并语义（cherry-pick 上游 PR #5417）

**Files:**
- Modify: `src-tauri/src/claude_desktop_config.rs`（由 cherry-pick 引入：`merge_profile` 函数 + 写入点改为读-合-写 + 两个测试）

**Interfaces:**
- Consumes: Task 1 的 cargo 工具链。
- Produces: `merge_profile(existing: &Value, new_profile: &Value) -> Value`（后续 Task 3 的注入发生在 merge 之前的 `profile` 上，故依赖 merge 已在写入点就位）；测试 `claude_desktop_apply_preserves_non_gateway_profile_fields`、`claude_desktop_apply_clears_stale_inference_models_when_new_provider_has_none`。

> 说明：这是导入一个上游已含测试的完整提交（PR #5417 的 `e71fe6998`），不重写 red-green。测试作为回归门跑通即视为补丁生效。若 cherry-pick 意外冲突（已用 `git apply --check --3way` 验证过能干净应用，理论上不会），按 PR diff 手工应用——diff 见 `gh pr diff 5417 --repo farion1231/cc-switch`。

- [ ] **Step 1: 拉取 PR head**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git fetch origin pull/5417/head:refs/remotes/origin/pr-5417
```

- [ ] **Step 2: cherry-pick 补丁**

```bash
git cherry-pick e71fe6998d4539036dd436ff330971a38326a7ca
git log --oneline -1   # 期望 HEAD 为 "fix(claude-desktop): preserve non-gateway profile fields ..."
```

- [ ] **Step 3: 跑相关测试**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
cargo test claude_desktop_apply   # 期望 3 个 test 全 PASS（2 新 + 1 既有）
```

- [ ] **Step 4: 确认提交已由 cherry-pick 创建（无需再 commit）**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git status   # 工作区干净
```

---

### Task 3: 补丁 B（后端）—— 显示名三键纳入自有字段

**Files:**
- Modify: `src-tauri/src/settings.rs`（新增 `ClaudeDesktopDisplaySettings` 结构体 + `AppSettings` 字段）
- Modify: `src-tauri/src/claude_desktop_config.rs`（新增 `inject_display_settings` + 在 `apply_provider_to_paths_inner` 接线）
- Test: `src-tauri/src/claude_desktop_config.rs`（`mod tests` 内新增两个测试）

**Interfaces:**
- Consumes: Task 2 的 `merge_profile` 与写入点结构（`let profile = match ...` 需改为 `let mut profile`）。
- Produces: `pub struct ClaudeDesktopDisplaySettings { name: String, subtitle: String, attribution: bool }`；`AppSettings.claude_desktop_display: Option<ClaudeDesktopDisplaySettings>`（serde camelCase → 前端字段名 `claudeDesktopDisplay`）；`fn inject_display_settings(profile: &mut Value, display: Option<&ClaudeDesktopDisplaySettings>)`。Task 4 前端依赖这些字段名与序列化约定。

- [ ] **Step 1: 在 settings.rs 定义结构体与字段**

在 `settings.rs` 的 `pub struct AppSettings {`（约 347 行，含 `#[serde(rename_all = "camelCase")]` 属性）**之前**插入结构体定义：

```rust
/// Claude Desktop 3P 左下角显示设置（deploymentDisplayName / deploymentDisplaySubtitle /
/// endUserAttribution 三键）。None = 功能关闭，profile 不写这三键（磁盘旧值由合并语义保留）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopDisplaySettings {
    pub name: String,
    pub subtitle: String,
    pub attribution: bool,
}
```

在 `AppSettings` 内、`pub language: Option<String>,` 字段之后插入：

```rust
    /// Claude Desktop 3P 左下角显示设置。None = 不接管这三键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_desktop_display: Option<ClaudeDesktopDisplaySettings>,
```

- [ ] **Step 2: 写失败测试（此时 `inject_display_settings` 不存在，编译失败 = red）**

在 `claude_desktop_config.rs` 的 `mod tests` 内、`use tempfile::TempDir;` 之后补一行 import，并在 tests mod 末尾追加两个测试：

```rust
    use crate::settings::ClaudeDesktopDisplaySettings;
```

```rust
    #[test]
    fn inject_display_settings_writes_keys_when_configured() {
        let mut profile = json!({ "inferenceProvider": "gateway" });
        let display = ClaudeDesktopDisplaySettings {
            name: "Chris".into(),
            subtitle: "Gateway".into(),
            attribution: false,
        };
        inject_display_settings(&mut profile, Some(&display));
        assert_eq!(profile["deploymentDisplayName"], json!("Chris"));
        assert_eq!(profile["deploymentDisplaySubtitle"], json!("Gateway"));
        assert_eq!(profile["endUserAttribution"], json!(false));
    }

    #[test]
    fn inject_display_settings_omits_keys_when_none() {
        let mut profile = json!({ "inferenceProvider": "gateway" });
        inject_display_settings(&mut profile, None);
        assert!(profile.get("deploymentDisplayName").is_none());
        assert!(profile.get("deploymentDisplaySubtitle").is_none());
        assert!(profile.get("endUserAttribution").is_none());
    }
```

- [ ] **Step 3: 运行测试确认失败**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
cargo test inject_display_settings   # 期望：编译错误（cannot find function `inject_display_settings`）
```

- [ ] **Step 4: 实现 `inject_display_settings` 并接线**

在 `claude_desktop_config.rs` 顶部 `use crate::database::...` 等 import 区附近加：

```rust
use crate::settings::{get_settings, ClaudeDesktopDisplaySettings};
```

在 `fn build_gateway_profile` 之前插入：

```rust
/// 把 Claude Desktop 左下角显示设置注入 profile（配置了才写）。
/// 未配置时不动 profile，三键由 merge 语义保留磁盘旧值。
fn inject_display_settings(
    profile: &mut Value,
    display: Option<&ClaudeDesktopDisplaySettings>,
) {
    if let Some(display) = display {
        profile["deploymentDisplayName"] = Value::String(display.name.clone());
        profile["deploymentDisplaySubtitle"] = Value::String(display.subtitle.clone());
        profile["endUserAttribution"] = Value::Bool(display.attribution);
    }
}
```

在 `apply_provider_to_paths_inner` 内：
- 把 `let profile = match provider_mode(provider) {` 改为 `let mut profile = match provider_mode(provider) {`
- 在 `match` 结束的 `};` 之后、`write_deployment_mode(&paths.normal_config_path, "3p")?;` 之前插入：

```rust
    inject_display_settings(&mut profile, get_settings().claude_desktop_display.as_ref());
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
cargo test inject_display_settings   # 期望 2 个 test PASS
cargo test claude_desktop_apply      # 期望补丁 A 的 3 个 test 仍 PASS（无回归）
```

- [ ] **Step 6: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git add src-tauri/src/settings.rs src-tauri/src/claude_desktop_config.rs
git commit -m "feat(claude-desktop): make 3P display name configurable

注入 deploymentDisplayName / deploymentDisplaySubtitle / endUserAttribution
三键到 gateway profile；None 时不写、由合并语义保留磁盘旧值。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 4: 补丁 B（前端）—— 设置项 UI

**Files:**
- Modify: `src/types.ts`（新增 `ClaudeDesktopDisplay` 接口 + `Settings` 字段）
- Create: `src/components/settings/ClaudeDesktopDisplaySettings.tsx`
- Modify: `src/components/settings/SettingsPage.tsx`（import + general tab 接线）
- Modify: `src/i18n/locales/en.json`、`src/i18n/locales/zh.json`（各加 6 个键）

**Interfaces:**
- Consumes: Task 1 的 `node_modules`（typecheck 依赖）；Task 3 的序列化约定——后端 `claude_desktop_display`（camelCase）→ 前端 `claudeDesktopDisplay`，`null` 表示关闭（serde `Option` 反序列化 null → None）。设置保存复用既有 `handleAutoSave`（全量表单保存，无需新命令）。
- Produces: `ClaudeDesktopDisplay` 前端类型；`ClaudeDesktopDisplaySettings` 组件（`value: ClaudeDesktopDisplay | null` + `onChange`）。Task 8 的验收依赖此 UI。

- [ ] **Step 1: 在 types.ts 加类型**

在 `src/types.ts` 的 `export interface Settings {` 附近（或文件末尾）加接口，并在 `Settings` 接口内加字段：

```ts
export interface ClaudeDesktopDisplay {
  name: string;
  subtitle: string;
  attribution: boolean;
}
```

在 `Settings` 接口内（如 `language` 字段之后）加：

```ts
  // Claude Desktop 3P 左下角显示设置（deploymentDisplayName / 副标题 / 归属开关）；null=关闭
  claudeDesktopDisplay?: ClaudeDesktopDisplay | null;
```

- [ ] **Step 2: 创建组件**

创建 `src/components/settings/ClaudeDesktopDisplaySettings.tsx`：

```tsx
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "react-i18next";
import type { ClaudeDesktopDisplay } from "@/types";

interface ClaudeDesktopDisplaySettingsProps {
  value: ClaudeDesktopDisplay | null;
  onChange: (value: ClaudeDesktopDisplay | null) => void;
}

export function ClaudeDesktopDisplaySettings({
  value,
  onChange,
}: ClaudeDesktopDisplaySettingsProps) {
  const { t } = useTranslation();
  const enabled = value !== null;

  const setEnabled = (on: boolean) => {
    onChange(on ? { name: "", subtitle: "", attribution: false } : null);
  };
  const patch = (p: Partial<ClaudeDesktopDisplay>) => {
    if (!value) return;
    onChange({ ...value, ...p });
  };

  return (
    <section className="space-y-3">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">
          {t("settings.claudeDesktopDisplay")}
        </h3>
        <p className="text-xs text-muted-foreground">
          {t("settings.claudeDesktopDisplayHint")}
        </p>
      </header>
      <div className="flex items-center justify-between">
        <Label htmlFor="cdd-enable" className="text-sm">
          {t("settings.claudeDesktopDisplayEnable")}
        </Label>
        <Switch id="cdd-enable" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {enabled && (
        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="cdd-name" className="text-sm">
              {t("settings.claudeDesktopDisplayName")}
            </Label>
            <Input
              id="cdd-name"
              value={value.name}
              onChange={(e) => patch({ name: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="cdd-subtitle" className="text-sm">
              {t("settings.claudeDesktopDisplaySubtitle")}
            </Label>
            <Input
              id="cdd-subtitle"
              value={value.subtitle}
              onChange={(e) => patch({ subtitle: e.target.value })}
            />
          </div>
          <div className="flex items-center justify-between">
            <Label htmlFor="cdd-attribution" className="text-sm">
              {t("settings.claudeDesktopDisplayAttribution")}
            </Label>
            <Switch
              id="cdd-attribution"
              checked={value.attribution}
              onCheckedChange={(v) => patch({ attribution: v })}
            />
          </div>
        </div>
      )}
    </section>
  );
}
```

- [ ] **Step 3: 加 i18n 键**

在 `src/i18n/locales/zh.json` 与 `src/i18n/locales/en.json` 的 `settings` 对象下各加（zh / en 两套）：

```jsonc
// zh.json
"claudeDesktopDisplay": "Claude Desktop 显示设置",
"claudeDesktopDisplayHint": "接管左下角显示名与归属开关（deploymentDisplayName 等三键）。关闭时不写入，保持 Claude Desktop 面板的现有值。",
"claudeDesktopDisplayEnable": "管理显示设置",
"claudeDesktopDisplayName": "显示名",
"claudeDesktopDisplaySubtitle": "副标题",
"claudeDesktopDisplayAttribution": "用户归属（endUserAttribution）"

// en.json
"claudeDesktopDisplay": "Claude Desktop display",
"claudeDesktopDisplayHint": "Take over the bottom-left display name and attribution (deploymentDisplayName and related keys). When off, keep the values already set in the Claude Desktop panel.",
"claudeDesktopDisplayEnable": "Manage display settings",
"claudeDesktopDisplayName": "Display name",
"claudeDesktopDisplaySubtitle": "Subtitle",
"claudeDesktopDisplayAttribution": "User attribution (endUserAttribution)"
```

（`ja.json` / `zh-TW.json` 不加，i18next 回退到 en。）

- [ ] **Step 4: 在 SettingsPage 接线**

在 `SettingsPage.tsx` 顶部 import 区加：

```tsx
import { ClaudeDesktopDisplaySettings } from "@/components/settings/ClaudeDesktopDisplaySettings";
```

在 general tab（`<TabsContent value="general">` 内，`LanguageSettings` 之后）插入：

```tsx
                    <ClaudeDesktopDisplaySettings
                      value={settings.claudeDesktopDisplay ?? null}
                      onChange={(v) =>
                        handleAutoSave({ claudeDesktopDisplay: v })
                      }
                    />
```

- [ ] **Step 5: typecheck**

```bash
cd "D:/Workspace/Project/cc-switch/src"
pnpm typecheck   # 期望：无错误
```

（前端无独立组件测试：逻辑在 Rust 侧已由 Task 3 单测覆盖，本任务以 `tsc --noEmit` + Task 8 运行验收把关。）

- [ ] **Step 6: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git add src/types.ts src/components/settings/ClaudeDesktopDisplaySettings.tsx src/components/settings/SettingsPage.tsx src/i18n/locales/en.json src/i18n/locales/zh.json
git commit -m "feat(ui): add Claude Desktop display settings section

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 5: 补丁 C（禁用 updater）+ 版本号 + 全量测试 + 构建

**Files:**
- Modify: `src-tauri/tauri.conf.json`（移除 updater 配置 + 版本号改为 3.20.3-local）
- Modify: `src-tauri/Cargo.toml`（version 改为 3.20.3-local）

**Interfaces:**
- Consumes: Task 1 的 `node_modules`；Task 2/3/4 的全部改动。
- Produces: NSIS 安装包 `src-tauri/target/release/bundle/nsis/CC Switch_3.20.3-local_x64-setup.exe`；已确认移除 updater 的产物。Task 6 部署依赖此产物。

- [ ] **Step 1: 移除 updater 配置并改版本号**

`src-tauri/tauri.conf.json` 三处改动：

1. 删除 `bundle` 下的 `"createUpdaterArtifacts": true`（整行；注意保持 JSON 合法——若它是 `bundle` 的最后一个键，需同时去掉前一键行末的逗号）
2. 删除 `plugins.updater` 整个对象（含 `pubkey` 与 `endpoints`）；若 `plugins` 下再无其他键，一并删除 `plugins`
3. `"version": "3.20.3"` → `"version": "3.20.3-local"`

`src-tauri/Cargo.toml`：`version = "3.20.3"` → `version = "3.20.3-local"`（`[package]` 段）。
（`package.json` 不动。）

- [ ] **Step 2: 校验 JSON 合法且 updater 配置确已移除**

```bash
cd "D:/Workspace/Project/cc-switch/src"
python -c "
import json
c = json.load(open('src-tauri/tauri.conf.json', encoding='utf-8'))
assert c['version'] == '3.20.3-local', c['version']
assert 'updater' not in c.get('plugins', {}), 'updater 配置仍在'
assert 'createUpdaterArtifacts' not in c.get('bundle', {}), 'createUpdaterArtifacts 仍在'
print('OK: version=%s, updater 已移除' % c['version'])
"
```

- [ ] **Step 3: 全量测试**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri"
cargo test   # 期望：全量 PASS。若有个别失败，须确认在干净 v3.20.3 上同样失败才可豁免（预存失败）
```

- [ ] **Step 4: 构建 NSIS 安装包**

```bash
cd "D:/Workspace/Project/cc-switch/src"
pnpm tauri build --bundles nsis   # 首次 15-40 分钟；无 updater 产物，不需要签名密钥
```

- [ ] **Step 5: 验证产物存在且官方 pubkey 已消失**

```bash
cd "D:/Workspace/Project/cc-switch/src"
ls -la "src-tauri/target/release/bundle/nsis/"   # 期望看到 *x64-setup.exe
# 官方 pubkey 前缀不应出现在产物二进制中（updater 配置已移除）：
grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEM4MDI4" \
  "src-tauri/target/release/cc-switch.exe"
# 期望输出 0
```

- [ ] **Step 6: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src"
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml
git commit -m "chore: disable updater in local build, bump to 3.20.3-local

本地构建移除 plugins.updater 与 createUpdaterArtifacts：lib.rs 在配置不完整时
跳过 updater 插件，官方更新无法覆盖本地修复。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 6: 部署（覆盖安装到现有目录）

**Files:** 无（系统级操作）

**Interfaces:**
- Consumes: Task 5 的 NSIS 安装包。
- Produces: 运行中的 CC Switch 3.20.3-local；官方 exe 备份；Task 7 验收的前提。

> 顺序说明：先退出再备份——运行中的 exe 被锁定无法复制。备份放在退出之后。

- [ ] **Step 1: 退出运行中的 CC Switch**

从系统托盘右键 → 退出（Quit/退出）。若托盘不可用，用优雅关闭（不带 `/F`）：

```bash
cmd //c "taskkill /IM cc-switch.exe"
```

确认进程已退：`tasklist | grep -i cc-switch` 应为空。

- [ ] **Step 2: 备份官方 exe**

```bash
cp "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" \
   "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe.official-3.20.3"
```

- [ ] **Step 3: 运行 NSIS 安装器，安装目录设为现有位置**

运行 Task 5 产出的 `CC Switch_3.20.3-local_x64-setup.exe`。**在安装向导里把安装目录设为 `C:\Users\Jason\AppData\Local\Programs\CC Switch`（覆盖现有目录，不新建）**——这样自动更新/自启项路径不变。若向导默认目录不同，浏览指向该目录。

（静默替代：`"…setup.exe" /S`，但静默会用默认目录，可能不落在 `Programs\CC Switch`，故推荐交互式安装。）

- [ ] **Step 4: 启动并确认版本**

启动 CC Switch，确认版本：

```bash
powershell -NoProfile -Command "(Get-Process cc-switch).Path + ' | ' + (Get-Process cc-switch).ProductVersion"
# 期望 ProductVersion = 3.20.3-local，Path = ...\Programs\CC Switch\cc-switch.exe
```

---

### Task 7: 验收第一步 —— 持久化（guard 仍在运行）

**Files:** 无（行为验收）

**Interfaces:**
- Consumes: Task 6 部署完成的 3.20.3-local。
- Produces: 持久化验收结论（通过 → 进入 Task 8）。

> 此阶段 guard 仍在运行，它与新构建写入的值一致（磁盘三键本就是 Chris/Gateway/false），不会冲突。

- [ ] **Step 1: 记基线**：确认 profile 当前 19 键（`%LOCALAPPDATA%\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json`）。

- [ ] **Step 2: 面板设开关（如关掉 Auto mode）→ CC Switch 切换供应商 → 面板值仍在。**

- [ ] **Step 3: 切回原供应商 → 路由跟随新供应商、开关值不变。**

- [ ] **Step 4: 重启 Claude Desktop → 值不变；左下角仍为 Chris / Gateway。**

- [ ] **Step 5: 判据检查**：profile 键数不再从 19 骤降到 7。任一步骤 profile 键数掉到 7 即判失败，停止并回到 Task 5 排查。

---

### Task 8: 删除 guard + 验收第二步 —— 可配置性

**Files:** 无（系统级操作 + 行为验收）

**Interfaces:**
- Consumes: Task 7 通过。
- Produces: guard 已卸载（备份保留）；显示名可配置性验证通过。

> 必须先卸载 guard 再测可配置性：guard 的 overlay 会把它记录的三键旧值「恢复」回去，与新配置值打架。

- [ ] **Step 1: 备份 guard 目录（保留可回退副本）**

```bash
cp -r "D:/Workspace/Claude Desktop/Code/Tmp/claude3p-profile-guard" \
      "D:/Workspace/Claude Desktop/Code/Tmp/claude3p-profile-guard.bak-preuninstall"
```

- [ ] **Step 2: 卸载 guard（停进程 + 清 HKCU Run 自启项）**

```bash
cd "D:/Workspace/Claude Desktop/Code/Tmp/claude3p-profile-guard"
python guard.py --uninstall
python guard.py --status   # 期望报「未安装/无守护」而非 HEALTHY
```

（若 `--uninstall` 会删除脚本文件，备份目录即为恢复副本：日后 `cp -r` 回来 + `python guard.py --install` 即可。）

- [ ] **Step 3: 启用显示设置**：CC Switch 设置 → 启用「管理 Claude Desktop 显示设置」→ 输入**与当前不同**的显示名（如 `Chris-Test`）→ 保存。

- [ ] **Step 4: 切换供应商 → `Chris-Test` 生效且其余字段不变。**

- [ ] **Step 5: 重启 Claude Desktop → `Chris-Test` 仍在。**

- [ ] **Step 6: 改回 `Chris` → 再次切换供应商 → 跟随新值**（证明可配置、非一次性）。

- [ ] **Step 7: 收尾确认**：`python guard.py --status` 不再守护；CC Switch 版本 3.20.3-local；profile 键数稳定。

---

## Self-Review 记录

- **Spec 覆盖**：合并语义（Task 2）、显示名三键可配置（Task 3 后端 + Task 4 前端）、删除 guard（Task 8）、版本号/构建/pubkey（Task 5）、部署（Task 6）、持久化验收（Task 7）、可配置性验收（Task 8）、工具链（Task 1）。风险项中的「14 个上游提交不含」「无签名 SmartScreen」「更新报签名错」为已知非阻塞，已写入 Global Constraints / 设计文档，无需额外任务。
- **占位符扫描**：无 TBD/TODO；所有代码步骤给出完整代码。
- **类型一致性**：`ClaudeDesktopDisplaySettings` 在 Task 3 定义、Task 4 前端以 `claudeDesktopDisplay`（camelCase）消费；`inject_display_settings` 签名 Task 3 定义并在其测试中使用；`merge_profile` 由 Task 2 引入、Task 3 依赖——跨任务命名一致。
