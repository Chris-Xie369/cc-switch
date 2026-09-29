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

// 临时本地定义：Task 6 会把 AggregateMaxEffort 加入 @/types 并从那里引入，届时删除本行。
type AggregateMaxEffort = "low" | "medium" | "high" | "xhigh" | "max";

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
