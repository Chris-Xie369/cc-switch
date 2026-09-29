# 聚合编辑器「诚实化」实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 spec `docs/superpowers/specs/2026-09-29-aggregate-editor-honesty-design.md` 落地四件事：sonnet 池连续化+本机迁移、三态思考档位徽标、maxEffort 入 UI、fork README+推送。

**Architecture:** 纯前端能力表（`claudeDesktopCapability.ts`）作为徽标与 maxEffort 联动的唯一事实源；Rust 侧仅透传字段（serde default 零迁移 + 写入时白名单过滤）；数据迁移沿用 2026-09-29 kimi 槽位改档的 DB+profile 双写手法。

**Tech Stack:** React + TS（vitest + testing-library）、Rust + Tauri v2（cargo test）、SQLite（CC Switch DB）、Claude Desktop 3P profile JSON。

## Global Constraints

- 分支 `fix/profile-merge`，只推 `origin`（Chris-Xie369/cc-switch），**绝不推 upstream**（farion1231/cc-switch）
- 提交信息**不含任何署名行**（无 Co-Authored-By 等）
- 回复与文档全程中文；DB/配置含真实密钥，**任何输出不复述密钥**
- cargo 不在默认 PATH：`export PATH="/c/Users/Jason/.cargo/bin:$PATH"`
- tauri build 必须带 `TAURI_BUNDLER_TOOLS_GITHUB_MIRROR` 且 `unset *_PROXY`
- 部署只经 `tools/install-local.bat`（Git Bash 会转写 NSIS 参数）；校验用 md5 + 前端资源名 grep，**不看版本号**
- 逆向事实源：Claude Desktop 2.9939.4.0 app.asar（`czt`/`lzt`/`szt`）；重验 grep 手册写入 `claudeDesktopCapability.ts` 注释与 README
- 工作目录：`D:/Workspace/Project/cc-switch/src`（git 仓库根）

---

### Task 1: 清点并分批提交存量改动 + 推送 origin

工作树上有一批本计划之前遗留的未提交改动（dropdown z-fix、池 keyed 重构、账本、
以及若干本会话之前就存在的零散修改）。先把家底清干净，后续任务才有干净的 diff。

**Files:**
- Modify: 仅 git 操作，不改文件内容

**Interfaces:**
- Produces: 干净的工作树；origin 上包含全部存量工作

- [ ] **Step 1: 清点现状**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git status --short && git log --oneline -3
```

- [ ] **Step 2: 逐个查看未提交 diff，归入主题**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git diff --stat && git diff src/components/ui/dropdown-menu.tsx src/utils/aggregateRoutes.ts src/utils/aggregateRoutes.test.ts | head -100
```

已知归属：
- `src/components/ui/dropdown-menu.tsx` + `tests/components/DropdownMenuZIndex.test.tsx` → **批次 A（z-fix）**
- `src/utils/aggregateRoutes.ts` + `src/utils/aggregateRoutes.test.ts` → **批次 B（池 keyed 重构）**
- `.superpowers/sdd/progress.md` → **批次 E（账本）**
- 其余（`forwarder.rs`、`aggregate.rs`、`services/provider/mod.rs`、`commands/upstream.rs`、`ClaudeDesktopProviderForm.tsx`、`AggregateProviderFields.tsx`、`UpdateBadge.tsx`、`AboutSection.tsx`、`UpdateContext.tsx`、`lib/updater.ts`）→ 逐个 `git diff <file>` 看内容归入 **批次 C/D**；若某文件 diff 与「OpenCode 会话头注入 / [1m] 剥离 / 相关表单联动」和「updater/关于页显示」两个主题都对不上，**停下来问用户**，不要猜。

- [ ] **Step 3: 批次 A 提交（z-fix）**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/components/ui/dropdown-menu.tsx tests/components/DropdownMenuZIndex.test.tsx && git commit -m "fix(ui): DropdownMenu 浮层提级 z-[100]，修复 FullScreenPanel 内下拉点不开"
```

- [ ] **Step 4: 批次 B 提交（池 keyed 重构）**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/utils/aggregateRoutes.ts src/utils/aggregateRoutes.test.ts && git commit -m "refactor(aggregate): RECOGNIZED_IDS 按槽位序号键入可留空位；溢出跳过池内撞名"
```

- [ ] **Step 5: 批次 C/D 按主题提交**

每个主题一个 commit（C = OpenCode 头注入与 [1m] 剥离相关 Rust/TS；D = updater/关于页）。
提交前 `git diff --staged` 复查无密钥、无署名行：

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add <批次C文件...> && git diff --staged | head -50 && git commit -m "fix(proxy): OpenCode 上游注入 x-opencode-session 与 User-Agent；[1m] 变体路由剥离"
```

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add <批次D文件...> && git commit -m "chore(ui): 更新器与关于页显示调整"
```

（提交信息按实际 diff 内容修正，不硬套。）

- [ ] **Step 6: 批次 E 提交（账本）**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add .superpowers/sdd/progress.md && git commit -m "docs: 账本——z-fix、池重构、kimi 槽位改档、k3-256k、1M 结论更正"
```

- [ ] **Step 7: 推送 origin**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git push origin fix/profile-merge
```

若连接被重置，走代理：

```bash
cd "D:/Workspace/Project/cc-switch/src" && git -c http.https://github.com.proxy=http://127.0.0.1:9674 push origin fix/profile-merge
```

Expected: `git status --short` 输出为空。

---

### Task 2: sonnet 池连续化（代码 + 测试）

**Files:**
- Modify: `src/utils/aggregateRoutes.ts`（RECOGNIZED_IDS 与模块注释）
- Test: `src/utils/aggregateRoutes.test.ts`

**Interfaces:**
- Consumes: 现有 `slotId(tier, ordinal, taken?)`（本文件，Task 1 后的 keyed 池版本）
- Produces: `slotId("sonnet", 3) === "claude-sonnet-5"`、`slotId("sonnet", 4) === "claude-sonnet-4"`（后续迁移与徽标依赖此序）

- [ ] **Step 1: 改测试（红）**

`src/utils/aggregateRoutes.test.ts` 三处改动：

① 「优先取真实 ID」用例中 `expect(slotId("sonnet", 4)).toBe("claude-sonnet-5")` 改为
`expect(slotId("sonnet", 3)).toBe("claude-sonnet-5")`；

② 整个删除用例「池子留空位时该序号走溢出（sonnet 池 3 位留空，保住存量 claude-sonnet-3）」，
其断言并入「池子用尽后退回」用例：

```ts
it("池子用尽后退回 claude-{档位}-{序号}", () => {
  expect(slotId("sonnet", 2)).toBe("claude-sonnet-4-5");
  expect(slotId("sonnet", 4)).toBe("claude-sonnet-4");
  expect(slotId("haiku", 2)).toBe("claude-haiku-2");
  expect(slotId("opus", 4)).toBe("claude-opus-4");
});
```

③ 「溢出让开池内已占用的名字」用例的注释改为
`（claude-sonnet-5 在池内，序号 5 顺移）`，断言不变（`slotId("sonnet", 5) === "claude-sonnet-6"`）。

- [ ] **Step 2: 跑测试确认红**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/aggregateRoutes.test.ts 2>&1 | tail -6
```

Expected: FAIL（sonnet 序号 3/4 的断言）

- [ ] **Step 3: 改实现**

`src/utils/aggregateRoutes.ts` 的 RECOGNIZED_IDS：

```ts
  // claude-sonnet-5 在 Desktop 精确表内且阶梯最全（low…xhigh…max）。
  sonnet: { 1: "claude-sonnet-4-6", 2: "claude-sonnet-4-5", 3: "claude-sonnet-5" },
```

同时删除模块注释块里「sonnet 3 位留空」的示例句
（`可以留空位……（sonnet 3 位留空即此）`改为通用表述`（如需为存量槽保 ID 可留空位）`）。

- [ ] **Step 4: 跑测试确认绿 + typecheck**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/aggregateRoutes.test.ts 2>&1 | tail -4 && npx tsc --noEmit 2>&1 | tail -2; echo "typecheck exit: $?"
```

Expected: 31 例全 PASS（含 assignSlotIds 批量唯一性），typecheck exit 0

- [ ] **Step 5: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/utils/aggregateRoutes.ts src/utils/aggregateRoutes.test.ts && git commit -m "refactor(aggregate): sonnet 池连续化——第 3 槽收 claude-sonnet-5"
```

---

### Task 3: 本机迁移（DB + defaultModel + profile）+ 重启回归

**Files:**
- Modify: CC Switch DB `providers` 表 rowid 57（`meta.aggregateRoutes`）、
  Claude Desktop profile `%LOCALAPPDATA%\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json`

**Interfaces:**
- Consumes: Task 2 的新池序（迁移后 DB 的槽位 ID 与编辑器重算结果一致）
- Produces: OC space-bunny 槽位 ID = `claude-sonnet-5`；`defaultModel` 引用同步改

- [ ] **Step 1: 备份 profile 并执行迁移（一个脚本，含断言）**

```bash
python << 'PYEOF'
import sqlite3, json, os, shutil
db = r'C:\Users\Jason\.cc-switch\cc-switch.db'
c = sqlite3.connect(db)
meta = json.loads(c.execute("select meta from providers where rowid=57").fetchone()[0])
agg = meta['aggregateRoutes']
hits = [s for s in agg['slots'] if s['routeId'] == 'claude-sonnet-3']
assert len(hits) == 1 and hits[0]['tier'] == 'sonnet', hits
hits[0]['routeId'] = 'claude-sonnet-5'
assert agg.get('defaultModel') == 'claude-sonnet-3', agg.get('defaultModel')
agg['defaultModel'] = 'claude-sonnet-5'
c.execute("update providers set meta=? where rowid=57", (json.dumps(meta, ensure_ascii=False),))
c.commit()

p = os.path.expandvars(r'%LOCALAPPDATA%\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json')
shutil.copy2(p, p + '.bak-sonnet5-continuous')
d = json.load(open(p, encoding='utf-8'))
m0 = [m for m in d['inferenceModels'] if m['name'] == 'claude-sonnet-3']
assert len(m0) == 1 and d['inferenceModels'][0]['name'] == 'claude-sonnet-3'
m0[0]['name'] = 'claude-sonnet-5'
json.dump(d, open(p, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
print('迁移完成: DB 槽位+defaultModel、profile 首条均已改名；备份', p + '.bak-sonnet5-continuous')
PYEOF
```

- [ ] **Step 2: 重启 CC Switch（轮询确认退出再启动）**

```bash
MSYS_NO_PATHCONV=1 taskkill /F /IM cc-switch.exe > /dev/null 2>&1; for i in $(seq 1 10); do tasklist 2>/dev/null | grep -qi "cc-switch.exe" || { echo "已退出"; break; }; sleep 2; done; cmd //c start "" "C:\Users\Jason\AppData\Local\Programs\CC Switch\cc-switch.exe" && sleep 10 && tasklist 2>/dev/null | grep -i "cc-switch.exe" | head -1
```

- [ ] **Step 3: 15 槽回归全 200**

```bash
python << 'PYEOF'
import json, os, urllib.request, urllib.error, time
PROFILE = os.path.expandvars(r'%LOCALAPPDATA%\Claude-3p\configLibrary\00000000-0000-4000-8000-000000157210.json')
BASE = "http://127.0.0.1:15721/claude-desktop"
token = json.load(open(PROFILE, encoding='utf-8'))['inferenceGatewayApiKey']
models = [m['name'] for m in json.load(open(PROFILE, encoding='utf-8'))['inferenceModels']]
def call(model):
    body = json.dumps({"model": model, "max_tokens": 8, "messages": [{"role":"user","content":"ping"}]}).encode()
    req = urllib.request.Request(BASE + "/v1/messages", data=body, method="POST",
        headers={"content-type":"application/json","authorization":f"Bearer {token}","x-api-key":token,"anthropic-version":"2023-06-01"})
    t=time.time()
    try:
        with urllib.request.urlopen(req, timeout=90) as r: return r.status, round(time.time()-t,1)
    except urllib.error.HTTPError as e: return e.code, round(time.time()-t,1)
    except Exception as e: return -1, str(e)[:50]
ok = 0
for m in models:
    st, el = call(m); ok += st == 200
    print(f"  {m:22s} -> {st}  {el}s")
print(f"通过 {ok}/{len(models)}")
assert 'claude-sonnet-5' in models and 'claude-sonnet-3' not in models
assert ok == len(models) == 15
PYEOF
```

Expected: 15/15 全 200，含 `claude-sonnet-5`、无 `claude-sonnet-3`

- [ ] **Step 4: 账本记录**

`.superpowers/sdd/progress.md` 追加条目：池连续化迁移（sonnet-3→sonnet-5、defaultModel 同步、15/15 回归）。
不单独提交，随 Task 9 推送。

---

### Task 4: `claudeDesktopCapability.ts` 三态能力表 + 测试

**Files:**
- Create: `src/utils/claudeDesktopCapability.ts`
- Test: `src/utils/claudeDesktopCapability.test.ts`

**Interfaces:**
- Consumes: `AggregateMaxEffort`（Task 6 加入 `src/types.ts`；本文件若先于 Task 6 执行，
  先在本文件定义 `EFFORT_LADDER` 常量，Task 6 时改为从 types 引入）
- Produces:
  - `effortCapability(routeId: string): "ladder" | "extended" | "none"`
  - `effortLevelsFor(routeId: string): readonly AggregateMaxEffort[]`（非 ladder 返回 `[]`）
  - `EFFORT_LADDER: readonly ["low","medium","high","xhigh","max"]`

- [ ] **Step 1: 写失败测试**

```ts
// src/utils/claudeDesktopCapability.test.ts
import { describe, expect, it } from "vitest";
import { effortCapability, effortLevelsFor } from "./claudeDesktopCapability";

describe("effortCapability（三态，镜像 Desktop asar czt/lzt/szt）", () => {
  it("完整阶梯：精确表 ID 与 fable/mythos 族", () => {
    expect(effortCapability("claude-sonnet-4-6")).toBe("ladder");
    expect(effortCapability("claude-sonnet-5")).toBe("ladder");
    expect(effortCapability("claude-opus-4-8")).toBe("ladder");
    expect(effortCapability("claude-fable-5")).toBe("ladder");
    expect(effortCapability("mythos-1")).toBe("ladder");
  });

  it("仅扩展思考开关：sonnet-4-5 与 haiku-4-5", () => {
    expect(effortCapability("claude-sonnet-4-5")).toBe("extended");
    expect(effortCapability("claude-haiku-4-5")).toBe("extended");
  });

  it("无：溢出 ID 与垃圾输入", () => {
    expect(effortCapability("claude-sonnet-3")).toBe("none");
    expect(effortCapability("claude-haiku-2")).toBe("none");
    expect(effortCapability("claude-haiku-3")).toBe("none");
    expect(effortCapability("claude-opus-4")).toBe("none");
    expect(effortCapability("garbage")).toBe("none");
  });

  it("剥 [1m] 后缀（大小写不敏感）与小写化后再判定", () => {
    expect(effortCapability("claude-sonnet-5[1m]")).toBe("ladder");
    expect(effortCapability("CLAUDE-FABLE-1[1M]")).toBe("ladder");
    expect(effortCapability("claude-sonnet-4-5[1m] ")).toBe("extended");
  });
});

describe("effortLevelsFor（阶梯明细，供 maxEffort 下拉）", () => {
  it("4-6 与 opus-4-6 无 xhigh；sonnet-5/opus-4-7/4-8 有", () => {
    expect(effortLevelsFor("claude-sonnet-4-6")).toEqual(["low","medium","high","max"]);
    expect(effortLevelsFor("claude-opus-4-6")).toEqual(["low","medium","high","max"]);
    expect(effortLevelsFor("claude-sonnet-5")).toContain("xhigh");
    expect(effortLevelsFor("claude-fable-1")).toEqual(["low","medium","high","xhigh","max"]);
  });

  it("非 ladder 返回空数组", () => {
    expect(effortLevelsFor("claude-haiku-4-5")).toEqual([]);
    expect(effortLevelsFor("claude-sonnet-3")).toEqual([]);
  });
});
```

- [ ] **Step 2: 跑测试确认红**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/claudeDesktopCapability.test.ts 2>&1 | tail -5
```

Expected: FAIL（模块不存在）

- [ ] **Step 3: 写实现**

```ts
// src/utils/claudeDesktopCapability.ts
/**
 * Claude Desktop 对模型 ID 的思考能力判定（三态）。
 *
 * 逆向自 Claude Desktop 2.9939.4.0 `app.asar`（混淆名 `czt`/`lzt`/`szt`，`$S` 小写化、
 * `eC` 剥尾部 `[1m]`）：
 *
 * ```js
 * n = czt[id] ?? (/^(?:claude-)?(?:fable|mythos)(?:-|$)/.test(id) ? szt : undefined)
 * // szt = 完整阶梯 low…xhigh…max；czt 内 sonnet-4-5 / haiku-4-5 仅 { modes:["extended"] }
 * ```
 *
 * **重验手册**（Desktop 升级后执行；输出表有变则同步更新本文件并跑实机徽标抽查）：
 *
 * ```bash
 * python - <<'PYEOF'
 * import glob
 * paths = sorted(glob.glob(r'C:\Program Files\WindowsApps\Claude_*\app\resources\app.asar'))
 * p = paths[-1]  # 多版本共存时须确认取到的是正在运行的版本：Get-Process Claude | Select Path
 * data = open(p, 'rb').read().decode('utf-8', 'replace')
 * i = data.find('czt={')
 * print(data[i:data.find('},lzt=', i) + 1])
 * PYEOF
 * ```
 */

import type { AggregateMaxEffort } from "@/types";

export type EffortCapability = "ladder" | "extended" | "none";

export const EFFORT_LADDER = ["low", "medium", "high", "xhigh", "max"] as const;

const EXACT_LADDERS: Record<string, readonly AggregateMaxEffort[]> = {
  "claude-sonnet-4-6": ["low", "medium", "high", "max"],
  "claude-sonnet-5": ["low", "medium", "high", "xhigh", "max"],
  "claude-opus-4-6": ["low", "medium", "high", "max"],
  "claude-opus-4-7": ["low", "medium", "high", "xhigh", "max"],
  "claude-opus-4-8": ["low", "medium", "high", "xhigh", "max"],
  // claude-opus-5 有 disallowThinkingDisabled（强制思考），聚合池不收录，但按表如实判定。
  "claude-opus-5": ["low", "medium", "high", "xhigh", "max"],
};

const EXTENDED_ONLY = new Set(["claude-haiku-4-5", "claude-sonnet-4-5"]);

const FAMILY_RE = /^(?:claude-)?(?:fable|mythos)(?:-|$)/;

function canonical(routeId: string): string {
  return routeId.trim().replace(/\[1m\]$/i, "").toLowerCase();
}

export function effortCapability(routeId: string): EffortCapability {
  const id = canonical(routeId);
  if (id in EXACT_LADDERS) return "ladder";
  if (EXTENDED_ONLY.has(id)) return "extended";
  return FAMILY_RE.test(id) ? "ladder" : "none";
}

export function effortLevelsFor(routeId: string): readonly AggregateMaxEffort[] {
  const id = canonical(routeId);
  if (id in EXACT_LADDERS) return EXACT_LADDERS[id];
  return FAMILY_RE.test(id) ? EFFORT_LADDER : [];
}
```

注意：`AggregateMaxEffort` 来自 Task 6 的 `src/types.ts`。若按顺序执行（本任务先跑），
在本文件顶部临时本地定义 `type AggregateMaxEffort = "low"|"medium"|"high"|"xhigh"|"max"`，
Task 6 完成后替换为 import 并删除本地定义。

- [ ] **Step 4: 跑测试确认绿**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/claudeDesktopCapability.test.ts 2>&1 | tail -4
```

Expected: 全 PASS

- [ ] **Step 5: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/utils/claudeDesktopCapability.ts src/utils/claudeDesktopCapability.test.ts && git commit -m "feat(aggregate): Desktop 思考能力三态判定表（徽标与 maxEffort 的事实源）"
```

---

### Task 5: 槽位行三态徽标 + 折叠摘要

**Files:**
- Modify: `src/components/providers/forms/AggregateProviderFields.tsx`
- Modify: `src/i18n/locales/zh.json`、`src/i18n/locales/en.json`（`aggregate` 段）
- Test: `tests/components/AggregateProviderFields.effortBadge.test.tsx`（新建）

**Interfaces:**
- Consumes: `effortCapability`（Task 4）
- Produces: 行内徽标组件 `EffortBadge`（本文件局部）；折叠摘要 `N 个模型 · M 强度`

- [ ] **Step 1: 加 i18n 键**

`src/i18n/locales/zh.json` 的 `"aggregate"` 对象内（紧挨 `"modelsSummary"`）加：

```json
"effortBadgeHeader": "思考",
"effortLadder": "强度✓",
"effortLadderTip": "完整思考强度阶梯（low…max），来自真模型 ID",
"effortToggle": "思考开关",
"effortToggleTip": "仅扩展思考开/关，无强度档位",
"effortNone": "✗",
"effortNoneTip": "溢出 ID：Claude Desktop 不认识，无思考控件",
```

`src/i18n/locales/en.json` 同位置加：

```json
"effortBadgeHeader": "Think",
"effortLadder": "Effort✓",
"effortLadderTip": "Full thinking-effort ladder (low…max) from a real model ID",
"effortToggle": "Toggle",
"effortToggleTip": "Extended-thinking on/off only, no effort levels",
"effortNone": "✗",
"effortNoneTip": "Overflow ID unknown to Claude Desktop; no thinking controls",
```

- [ ] **Step 2: 写失败组件测试**

```tsx
// tests/components/AggregateProviderFields.effortBadge.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AggregateProviderFields } from "@/components/providers/forms/AggregateProviderFields";
import type { AggregateRoutes, Provider } from "@/types";

function makeRoutes(pairs: Array<[string, "sonnet" | "haiku"]>): AggregateRoutes {
  return {
    slots: pairs.map(([routeId, tier], i) => ({
      routeId, tier, providerId: "p1",
      upstreamModel: `m${i}`, supports1m: false,
    })),
    defaultTarget: { kind: "providerId", value: "p1" },
  };
}

function renderEditor(value: AggregateRoutes) {
  return render(
    <AggregateProviderFields
      value={value}
      onChange={vi.fn()}
      candidates={[{ id: "p1", name: "P1" } as unknown as Provider]}
      modelsForProvider={() => []}
      fetchingProviderId={null}
      onFetchModels={vi.fn()}
    />,
  );
}

describe("聚合编辑器三态思考徽标", () => {
  it("展开卡后 ladder 槽显示 强度✓、溢出槽显示 ✗", async () => {
    const user = userEvent.setup();
    renderEditor(makeRoutes([["claude-sonnet-5", "sonnet"], ["claude-haiku-3", "haiku"]]));
    await user.click(screen.getByRole("button", { expanded: false })); // 展开唯一的卡
    expect(screen.getByText("强度✓")).toBeInTheDocument();
    expect(screen.getByText("✗")).toBeInTheDocument();
  });

  it("extended 槽显示 思考开关", async () => {
    const user = userEvent.setup();
    renderEditor(makeRoutes([["claude-sonnet-4-5", "sonnet"]]));
    await user.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("思考开关")).toBeInTheDocument();
  });

  it("折叠摘要含强度计数", () => {
    renderEditor(makeRoutes([["claude-sonnet-5", "sonnet"], ["claude-haiku-3", "haiku"]]));
    expect(screen.getByText("2 个模型 · 1 强度")).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: 跑测试确认红**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run tests/components/AggregateProviderFields.effortBadge.test.tsx 2>&1 | tail -5
```

Expected: FAIL（找不到「强度✓」）

- [ ] **Step 4: 改组件**

`AggregateProviderFields.tsx`：

① import 区加：

```ts
import { effortCapability } from "@/utils/claudeDesktopCapability";
```

② 文件底部（主组件外）加徽标组件：

```tsx
function EffortBadge({ routeId }: { routeId: string }) {
  const { t } = useTranslation();
  // 测试环境 i18n 是空资源：t() 必须带 defaultValue，否则返回键名本身、组件测试必挂
  const conf = {
    ladder: { key: "effortLadder", tipKey: "effortLadderTip", cls: "text-emerald-600 dark:text-emerald-400", text: "强度✓", tip: "完整思考强度阶梯（low…max），来自真模型 ID" },
    extended: { key: "effortToggle", tipKey: "effortToggleTip", cls: "text-muted-foreground", text: "思考开关", tip: "仅扩展思考开/关，无强度档位" },
    none: { key: "effortNone", tipKey: "effortNoneTip", cls: "text-muted-foreground/50", text: "✗", tip: "溢出 ID：Claude Desktop 不认识，无思考控件" },
  }[effortCapability(routeId)];
  return (
    <span
      className={`w-16 shrink-0 text-[10px] ${conf.cls}`}
      title={t(`aggregate.${conf.tipKey}`, { defaultValue: conf.tip })}
    >
      {t(`aggregate.${conf.key}`, { defaultValue: conf.text })}
    </span>
  );
}
```

③ 列头行（`{/* 列头：紧贴供应商行 */}` 那段）在 tier `<span className="w-14 ...">` 后
插入徽标列占位，保持列对齐：

```tsx
<span className="w-14 shrink-0">{t("aggregate.tier")}</span>
<span className="w-16 shrink-0">{t("aggregate.effortBadgeHeader")}</span>
```

④ 槽位行（`mappedTiers.map` 内）同样在 tier `<span className="w-14 ...">{tier}</span>` 后加：

```tsx
<EffortBadge routeId={slot.routeId} />
```

⑤ 折叠态摘要（`{/* 折叠态摘要 */}` 段）整体替换为：

```tsx
<p className="text-xs text-muted-foreground">
  {t("aggregate.modelsSummaryLadder", {
    count: mappedTiers.length,
    ladder: mappedTiers.filter((tier) => effortCapability(card.rows[tier]!.routeId) === "ladder").length,
    defaultValue: "{{count}} 个模型 · {{ladder}} 强度",
  })}
</p>
```

⑥ i18n：`zh.json` 的 aggregate 段加 `"modelsSummaryLadder": "{{count}} 个模型 · {{ladder}} 强度"`，
`en.json` 加 `"modelsSummaryLadder": "{{count}} models · {{ladder}} effort"`。
然后 `grep -rn "aggregate.modelsSummary" src/ tests/`——若旧键 `modelsSummary` 已无使用者，
从两个语言文件删除。

- [ ] **Step 5: 跑测试确认绿 + typecheck**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run tests/components/AggregateProviderFields.effortBadge.test.tsx 2>&1 | tail -4 && npx tsc --noEmit 2>&1 | tail -2; echo "exit: $?"
```

Expected: 3 例 PASS，typecheck exit 0

- [ ] **Step 6: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/components/providers/forms/AggregateProviderFields.tsx src/i18n/locales/zh.json src/i18n/locales/en.json tests/components/AggregateProviderFields.effortBadge.test.tsx && git commit -m "feat(aggregate): 槽位行三态思考徽标 + 折叠摘要强度计数"
```

---

### Task 6: maxEffort 数据层（TS 类型 + Rust 字段/过滤/写入）

**Files:**
- Modify: `src/types.ts`（`AggregateRouteSlot`）
- Modify: `src-tauri/src/aggregate.rs`（`AggregateRouteSlot`、`ResolvedModelRoute`、resolve 过滤、测试）
- Modify: `src-tauri/src/claude_desktop_config.rs`（profile 条目写入 + 测试）

**Interfaces:**
- Consumes: 现有 `AggregateRouteSlot`（serde camelCase）、`ResolvedModelRoute`、
  profile 条目构建处（`claude_desktop_config.rs` 内 grep `labelOverride` 定位）
- Produces:
  - TS: `AggregateRouteSlot.maxEffort?: AggregateMaxEffort`；
    `export type AggregateMaxEffort = "low"|"medium"|"high"|"xhigh"|"max"`
  - Rust: `AggregateRouteSlot.max_effort: Option<String>`（serde default）、
    `ResolvedModelRoute.max_effort: Option<String>`（非法值过滤后）

- [ ] **Step 0: asar 复核 maxEffort 的条目字段形状（spec 风险表要求）**

```bash
python - <<'PYEOF'
import subprocess, re
out = subprocess.run(["powershell","-NoProfile","-Command",
  "(Get-Process Claude | Select-Object -First 1 -ExpandProperty Path)"],
  capture_output=True, text=True).stdout.strip()
asar = out[:out.rfind("\\")] + r"\resources\app.asar"
print("asar:", asar)
data = open(asar, 'rb').read().decode('utf-8', 'replace')
for m in list(re.finditer(r'maxEffort', data))[:6]:
    s = m.start(); print(repr(data[max(0,s-120):s+120])); print('-'*60)
PYEOF
```

Expected: 确认 `inferenceModels` 条目解析路径上存在 per-entry `maxEffort`
（已知证据：`{...t.plain,supports1m:!0,...r&&{maxEffort:r}}` 的折叠取值逻辑）。
若形状不同（如顶层键），只调整 Step 5 的写入目标，TS/Rust 字段定义不变。

- [ ] **Step 1: TS 类型**

`src/types.ts` 的 `AggregateRouteSlot` 接口加一行，并在其上方加导出类型：

```ts
export type AggregateMaxEffort = "low" | "medium" | "high" | "xhigh" | "max";
```

```ts
  /** 思考强度上限（仅完整阶梯 ID 有意义；写入 profile 时透出为 maxEffort） */
  maxEffort?: AggregateMaxEffort;
```

- [ ] **Step 2: Rust — 字段与过滤（先加测试，红）**

`src-tauri/src/aggregate.rs` tests 模块加（serde 断言）：

```rust
#[test]
fn max_effort_absent_defaults_to_none_and_survives_roundtrip() {
    let plain: AggregateRouteSlot = serde_json::from_str(
        r#"{"routeId":"claude-sonnet-5","tier":"sonnet","providerId":"p","upstreamModel":"m","supports1m":false}"#,
    )
    .unwrap();
    assert_eq!(plain.max_effort, None);

    let with: AggregateRouteSlot = serde_json::from_str(
        r#"{"routeId":"claude-sonnet-5","tier":"sonnet","providerId":"p","upstreamModel":"m","supports1m":false,"maxEffort":"xhigh"}"#,
    )
    .unwrap();
    assert_eq!(with.max_effort.as_deref(), Some("xhigh"));
    assert!(serde_json::to_string(&with).unwrap().contains(r#""maxEffort":"xhigh""#));
}
```

跑 `cargo test max_effort`（在 `src-tauri/` 下，先 export PATH）确认编译失败（字段不存在）。

- [ ] **Step 3: Rust — 实现**

`aggregate.rs`：

① `AggregateRouteSlot` 加字段：

```rust
    /// 思考强度上限（camelCase: maxEffort）。仅完整阶梯 ID 有意义。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_effort: Option<String>,
```

② 文件级常量：

```rust
const AGGREGATE_MAX_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
```

③ `ResolvedModelRoute` 加 `pub max_effort: Option<String>`；
`resolve` 的 `out.push(ResolvedModelRoute { ... })` 处加：

```rust
            max_effort: slot
                .max_effort
                .as_deref()
                .filter(|v| AGGREGATE_MAX_EFFORTS.contains(v))
                .map(str::to_string),
```

④ `grep -rn "ResolvedModelRoute {" src-tauri/src/`——除上一步的 push 处外，
所有字面量（含测试辅助，已知 `proxy/server.rs` 有 `supports_1m: None` 字面量）补
`max_effort: None`。`grep -rn "AggregateRouteSlot {" src-tauri/src/` 同理
（`aggregate.rs` 的 `slot()`/`grouped()` 测试辅助等）。

⑤ `claude_desktop_config.rs`：`grep -n "labelOverride" src-tauri/src/claude_desktop_config.rs`
定位 profile `inferenceModels` 条目构建处，紧挨 `supports1m` 写入之后加：

```rust
        if let Some(effort) = route.max_effort.as_deref() {
            entry["maxEffort"] = serde_json::json!(effort);
        }
```

（`entry` 换成实际构建的变量名；若条目用 `json!` 宏一次性构造，则改为构造后追加。）

⑥ 扩展既有测试 `aggregate_provider_derives_model_routes_from_slots` 的 Harness：
复制该测试里「构造 slots → 断言 routes」的写法，新增一条带 `"maxEffort":"xhigh"`
的 sonnet 槽，断言生成的 profile 条目含 `"maxEffort":"xhigh"`；再断言带
`"maxEffort":"ultra"`（非法值）的槽产出条目**不含** `maxEffort` 键。

- [ ] **Step 4: 跑 Rust 测试确认绿**

```bash
cd "D:/Workspace/Project/cc-switch/src/src-tauri" && export PATH="/c/Users/Jason/.cargo/bin:$PATH" && cargo test aggregate 2>&1 | tail -6
```

Expected: aggregate 相关全 PASS（含新 max_effort 用例）

- [ ] **Step 5: TS 侧保留性测试**

`src/utils/aggregateRoutes.test.ts` 加：

```ts
it("assignSlotIds 保留 maxEffort 字段", () => {
  const routes = {
    slots: [
      { routeId: "claude-sonnet-5", tier: "sonnet" as const, providerId: "p",
        upstreamModel: "m", supports1m: false, maxEffort: "xhigh" as const },
    ],
    defaultTarget: { kind: "providerId" as const, value: "p" },
  };
  expect(assignSlotIds(routes).slots[0].maxEffort).toBe("xhigh");
});
```

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run src/utils/aggregateRoutes.test.ts 2>&1 | tail -4 && npx tsc --noEmit 2>&1 | tail -2; echo "exit: $?"
```

Expected: PASS，typecheck exit 0

- [ ] **Step 6: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/types.ts src/utils/aggregateRoutes.test.ts src-tauri/src/aggregate.rs src-tauri/src/claude_desktop_config.rs && git commit -m "feat(aggregate): maxEffort 字段贯通——TS 类型/Rust serde/写入白名单过滤"
```

---

### Task 7: maxEffort 编辑器下拉 + 提交归一化

**Files:**
- Modify: `src/components/providers/forms/AggregateProviderFields.tsx`
- Modify: `src/i18n/locales/zh.json`、`src/i18n/locales/en.json`

**Interfaces:**
- Consumes: `effortCapability`、`effortLevelsFor`（Task 4）、`AggregateMaxEffort`（Task 6）、
  现有 `patchRow`、`UNSET` 常量（`"__unset__"`，本文件已有）
- Produces: 行内「上限」下拉（仅 ladder 可选）；`commit()` 对非 ladder 槽剥离 maxEffort

- [ ] **Step 1: i18n 键**

`zh.json` aggregate 段：`"maxEffort": "上限"`, `"maxEffortOff": "不限制"`
`en.json` aggregate 段：`"maxEffort": "Cap"`, `"maxEffortOff": "Unlimited"`

- [ ] **Step 2: 组件改动**

`AggregateProviderFields.tsx`：

① import：

```ts
import { effortCapability, effortLevelsFor } from "@/utils/claudeDesktopCapability";
import type { AggregateMaxEffort } from "@/types";
```

② 文件底部加下拉组件：

```tsx
function MaxEffortSelect({
  routeId,
  value,
  onChange,
}: {
  routeId: string;
  value: AggregateMaxEffort | undefined;
  onChange: (v: AggregateMaxEffort | undefined) => void;
}) {
  const { t } = useTranslation();
  const disabled = effortCapability(routeId) !== "ladder";
  return (
    <Select
      value={value ?? UNSET}
      disabled={disabled}
      onValueChange={(v) => onChange(v === UNSET ? undefined : (v as AggregateMaxEffort))}
    >
      <SelectTrigger
        className="h-7 w-24 shrink-0 text-xs"
        title={
          disabled
            ? t("aggregate.effortNoneTip", {
                defaultValue: "溢出 ID：Claude Desktop 不认识，无思考控件",
              })
            : undefined
        }
      >
        <SelectValue
          placeholder={t("aggregate.maxEffort", { defaultValue: "上限" })}
        />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={UNSET}>
          {t("aggregate.maxEffortOff", { defaultValue: "不限制" })}
        </SelectItem>
        {effortLevelsFor(routeId).map((lv) => (
          <SelectItem key={lv} value={lv}>
            {lv}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
```

③ 列头行在「显示名」`<span className="min-w-0 flex-1">` 之后、`1M 上下文` 之前加：

```tsx
<span className="w-24 shrink-0 text-right">{t("aggregate.maxEffort")}</span>
```

④ 槽位行同位置（显示名 `</div>` 之后、1M `<div className="flex w-20 ...">` 之前）加：

```tsx
<MaxEffortSelect
  routeId={slot.routeId}
  value={slot.maxEffort}
  onChange={(v) => patchRow(cardIndex, tier, { maxEffort: v })}
/>
```

⑤ `commit` 归一化（非 ladder 槽剥离 maxEffort）。**必须先 `assignSlotIds` 再判定**：
槽位增删会让序号轮换、ID 在溢出↔池内之间变档，用旧 ID 判定会剥错方向：

```ts
const commit = (next: AggregateRoutes) => {
  const assigned = assignSlotIds(next);
  return onChange({
    ...assigned,
    slots: assigned.slots.map((s) =>
      effortCapability(s.routeId) === "ladder"
        ? s
        : { ...s, maxEffort: undefined },
    ),
  });
};
```

- [ ] **Step 3: 扩展组件测试**

`tests/components/AggregateProviderFields.effortBadge.test.tsx` 加：

```tsx
it("上限下拉：ladder 槽可选，溢出槽禁用", async () => {
  const user = userEvent.setup();
  renderEditor(makeRoutes([["claude-sonnet-5", "sonnet"], ["claude-haiku-3", "haiku"]]));
  await user.click(screen.getByRole("button", { expanded: false }));
  const triggers = screen.getAllByRole("combobox");
  expect(triggers).toHaveLength(2);
  expect(triggers[0]).toBeEnabled();
  expect(triggers[1]).toBeDisabled();
});
```

（若 Radix Select 触发器角色断言不稳，改用 `container.querySelectorAll("button[aria-haspopup='listbox']")`
取两个触发器后断言 disabled 属性。）

- [ ] **Step 4: 跑测试 + typecheck + 全量前端回归**

```bash
cd "D:/Workspace/Project/cc-switch/src" && npx vitest run tests/components/AggregateProviderFields.effortBadge.test.tsx 2>&1 | tail -4 && npx tsc --noEmit 2>&1 | tail -2 && npx vitest run 2>&1 | tail -4
```

Expected: 新用例 PASS；typecheck 0；全量仅已知 flaky（`PiProviderForm`/`App.test.tsx`
并发下偶发——单跑通过即可，参考 2026-09-29 z-fix 验证结论）

- [ ] **Step 5: 提交**

```bash
cd "D:/Workspace/Project/cc-switch/src" && git add src/components/providers/forms/AggregateProviderFields.tsx src/i18n/locales/zh.json src/i18n/locales/en.json tests/components/AggregateProviderFields.effortBadge.test.tsx && git commit -m "feat(aggregate): 槽位行 maxEffort 下拉（仅完整阶梯可选）+ 提交归一化"
```

---

### Task 8: 构建部署 + 15 槽回归 + 实机验收

**Files:**
- 无代码改动；产物部署与验收

- [ ] **Step 1: 构建（后台，15 分钟量级）**

```bash
cd "D:/Workspace/Project/cc-switch/src" && export PATH="/c/Users/Jason/.cargo/bin:$PATH" && export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR="https://gh-proxy.com/https://github.com/" && unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy && pnpm tauri build --bundles nsis 2>&1 | tail -5
```

Expected: exit 0，产物 `src-tauri/target/release/bundle/nsis/CC Switch_3.20.4-local_x64-setup.exe`

- [ ] **Step 2: 停进程 → 安装 → 校验 → 启动**

```bash
MSYS_NO_PATHCONV=1 taskkill /F /IM cc-switch.exe > /dev/null 2>&1; for i in $(seq 1 10); do tasklist 2>/dev/null | grep -qi "cc-switch.exe" || { echo "已退出"; break; }; sleep 2; done; MSYS_NO_PATHCONV=1 cmd /c "D:\Workspace\Project\cc-switch\tools\install-local.bat" 2>&1 | tail -3
```

```bash
cd "D:/Workspace/Project/cc-switch/src" && md5sum src-tauri/target/release/cc-switch.exe "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" && grep -a -c "$(ls dist/assets/ | grep '^index-.*\.js$')" "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" && grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6" "C:/Users/Jason/AppData/Local/Programs/CC Switch/cc-switch.exe" || true
```

Expected: 两个 md5 一致；资源名命中 1；pubkey 计数 0

```bash
cmd //c start "" "C:\Users\Jason\AppData\Local\Programs\CC Switch\cc-switch.exe" && sleep 8 && tasklist 2>/dev/null | grep -i "cc-switch.exe" | head -1
```

- [ ] **Step 3: 15 槽回归**（复用 Task 3 Step 3 的脚本，断言 15/15 全 200）

- [ ] **Step 4: 实机验收清单（需要用户在 GUI 操作，逐项确认）**

1. 编辑 MoA → 5 张卡徽标与 asar 表一致（抽查易错点：Zhipu sonnet 显示「思考开关」、
   OC sonnet 显示「强度✓」[Task 3 迁移后]、Kimi haiku 显示「✗」）
2. Kimi fable 槽「上限」下拉选 `high` → 保存 → 重启 CC Switch →
   profile 里 `claude-fable-5` 条目出现 `"maxEffort": "high"`
3. OC sonnet 槽（溢出→现已 ladder）上限下拉可选且含 xhigh
4. Zhipu sonnet 槽上限下拉禁用（extended 槽）

- [ ] **Step 5: 账本记录实施结果**

`.superpowers/sdd/progress.md` 追加：四项交付、回归结果、验收结论。

---

### Task 9: README + 最终推送

**Files:**
- Create: `src/README.md`（fork 仓库根）

**Interfaces:**
- Consumes: UPSTREAM-SYNC.md §5–§6 的构建/部署命令（引用不复制维护细节）、
  Task 4 的重验手册（互引）

- [ ] **Step 1: 写 README**

```markdown
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
- 同步流程、补丁清单、冲突热点见 [UPSTREAM-SYNC.md](UPSTREAM-SYNC.md)

## Windows 构建与部署

工具链：Rust 1.95+（cargo 需手动进 PATH）、node 24+、pnpm 10+。

​```bash
cd <repo>
export PATH="/c/Users/Jason/.cargo/bin:$PATH"
export TAURI_BUNDLER_TOOLS_GITHUB_MIRROR="https://gh-proxy.com/https://github.com/"
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy
pnpm install          # 依赖有变时
pnpm tauri build --bundles nsis
​```

- 镜像与绕代理是**必需**的：tauri CLI 经 9674 代理下载 NSIS 工具会被 gh-proxy 拒 400
- 产物：`src-tauri/target/release/bundle/nsis/CC Switch_<版本>-local_x64-setup.exe`
- 部署经 `tools/install-local.bat`（Git Bash 直跑安装器会转写 `/S` `/D=` 参数）；
  装前必须轮询确认旧进程退出（exe 被占用时 NSIS 静默失败且 EXITCODE=0）
- 装后校验（**不看版本号**）：`md5sum` 对比 `target/release/cc-switch.exe`；
  `grep -a -c "<dist/assets/index-*.js 名>" <安装 exe>` 应为 1（确认前端也换了）；
  `grep -a -c "dW50cnVzdGVkIGNvbW1lbnQ6"` 应为 0（不含官方签名公钥）

## 已知耦合：Claude Desktop 黑盒知识（升级后须重验）

聚合槽位的**思考能力**由 Claude Desktop 对模型 ID 的判定决定（逆向自 2.9939.4.0）：

| 能力 | ID | 表现 |
|---|---|---|
| 完整强度阶梯 | `claude-sonnet-4-6/5`、`claude-opus-4-6/4-7/4-8`、fable/mythos 族 | low…max 滑条 |
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
```

（正文中的 `​```bash` 转义写进 README 时去掉零宽字符，就是普通代码围栏。）

- [ ] **Step 2: 提交 README + 刷新账本快照**

（注意：`.superpowers/sdd/progress.md` 被 `.superpowers/` 的 gitignore 忽略、不能直接
add——Task 1 实测确认。账本入库走快照机制：cp 到 `docs/local-maintenance/progress.md`。）

```bash
cd "D:/Workspace/Project/cc-switch/src" && cp .superpowers/sdd/progress.md docs/local-maintenance/progress.md && git add README.md docs/local-maintenance/progress.md && git commit -m "docs: fork README（构建/部署/已知耦合/怪癖/验收口径）+ 账本快照刷新"
```

- [ ] **Step 3: 推送 origin**（命令同 Task 1 Step 7，含代理 fallback）

Expected: `git status --short` 为空；origin HEAD == 本地 HEAD

- [ ] **Step 4: 收尾汇报**

向用户汇报：四项交付状态、回归/验收结果、推送哈希。
