import type {
  AggregateRouteSlot,
  AggregateRoutes,
  AggregateTier,
  Provider,
} from "@/types";
import { effortLevelsFor } from "./claudeDesktopCapability";

/**
 * 每个档位的 **Claude Desktop 认得**的真模型 ID 池，按优先级排列。
 *
 * ID 决定了模型在 Claude Desktop 里的**能力画像**，其中一项是「有没有推理强度选择」。
 * 实测（2.2553.1.0，`app.asar` 内 `lPt(id)`）：
 *
 * ```js
 * lPt = (id) => XNt[qAt(id)] ?? (ZNt.test(id) ? YNt : undefined)   // 都不中 → 没有强度控件
 * XNt = { "claude-opus-4-8": {effortLevels:[low…max]}, … }          // 精确表：真 Anthropic ID
 * ZNt = /^(?:claude-)?(?:fable|mythos)(?:-|$)/                      // fable/mythos 族走正则
 * ```
 *
 * 也就是说 `claude-opus-1` 这种自造 ID 既不在表里、也不匹配该正则，**整条模型就没有
 * 推理强度控件**（而 `claude-fable-*` 反而有——它命中了族正则）。用户此前手写的
 * profile 也是这个套路：`name` 用真 ID（`claude-opus-4-8`）承载能力画像，`labelOverride`
 * 才是给人看的「实际调用的模型」。故这里按档位给出 ID 池，序号逐个取用。
 *
 * 阶梯明细见 claudeDesktopCapability.ts（EXACT_LADDERS）：`4-8`/`4-7` 含 xhigh，
 * `4-6` 无 xhigh 但有阶梯（低版本 asar 曾只有 extended 开关，已过时）；不取
 * `claude-opus-5`（它 `disallowThinkingDisabled`，会强制开启思考）。池子用尽后退回 `claude-{档位}-{序号}`：形状合法、可路由，
 * 只是那条不再有强度控件。
 *
 * 池 keyed by 槽位序号（该档位第几个槽），可以留空位——空位序号走溢出，
 * 用于「存量槽已占溢出 ID、新槽拿真 ID」的场景（如需为存量槽保 ID 可留空位）。
 *
 * 池内 ID 形如 `claude-{档位}-{数字}` 时会与溢出生成值撞名（`claude-sonnet-5` 踩过：
 * 同档第 5 个槽位也生成 `claude-sonnet-5`，两条槽位同 ID，后端去重吃掉一条），
 * 故 slotId 的溢出路径会跳过池内已占用的名字；assignSlotIds 批量分配时再传一个
 * 本次已分配的集合，溢出同时让开同伴（序号 5 让出 sonnet-5 取 6 后，序号 6 取 7）。
 */
const RECOGNIZED_IDS: Record<
  AggregateTier,
  Readonly<Partial<Record<number, string>>>
> = {
  opus: { 1: "claude-opus-4-8", 2: "claude-opus-4-7", 3: "claude-opus-4-6" },
  // claude-sonnet-5 在 Desktop 精确表内且阶梯最全（low…xhigh…max）。
  // 序号 4 固化 claude-sonnet-3：claude-sonnet-4 已被 CLI 目录退役（2026-06-15），
  // 与目录条目撞名的 ID 会被 /model 选择器过滤；合成名经 behavesAs 永远可显示。
  sonnet: {
    1: "claude-sonnet-4-6",
    2: "claude-sonnet-4-5",
    3: "claude-sonnet-5",
    4: "claude-sonnet-3",
  },
  haiku: { 1: "claude-haiku-4-5" },
  // fable 族的强度阶梯由族正则兜底，序号可无限生成，通常无需 ID 池。
  // 序号 5 固化 claude-fable-6：claude-fable-5 被 CLI 目录降级进 overflow 区
  // （继任 Fable 5.1 占 main），同名行在 /model 选择器被压走；合成名无此问题。
  fable: { 5: "claude-fable-6" },
};

/**
 * 生成槽位 ID：优先取该档位的真模型 ID，用尽后退回 `claude-{档位}-{序号}`。
 *
 * 这个字符串就是 Claude Desktop 模型列表里的 `inferenceModels[].name`，也是请求按
 * 模型分流的键，因此**必须过 Claude Desktop 的校验**。实测（2.2553.1.0）它的校验是
 * 「厂商词黑名单 + Anthropic 形状」：
 *
 * ```js
 * Vxe = /ark-code|…|deepseek|glm|gpt|gemini|grok|kimi|qwen|…/   // 厂商词
 * Go  = (name) => Vxe.test(name) ? false : Wo.test(name) || name.includes("claude")
 * ```
 *
 * 名字里只要出现 deepseek/glm 这类厂商词就会被判为 "is not an Anthropic model" 并从
 * 列表移除，**且是整组生效**——旧方案把供应商名 slug 进 ID（`claude-fable-deepseek`、
 * `claude-fable-zhipu-glm`），四个槽位被删光、选择器变空。故 ID 与供应商名无关，
 * 可读性交给「显示名」（`labelOverride`，不受该校验约束）。
 */
export function slotId(
  tier: AggregateTier,
  ordinal: number,
  taken: ReadonlySet<string> = new Set(),
): string {
  const pooled = RECOGNIZED_IDS[tier][ordinal];
  if (pooled) return pooled;
  const poolValues = new Set(Object.values(RECOGNIZED_IDS[tier]));
  let n = ordinal;
  let candidate = `claude-${tier}-${n}`;
  // 溢出：跳过池内已占用的名字（如 sonnet-5 入池后序号 5 顺移）以及与同批
  // 已分配同伴的撞名（assignSlotIds 传入 taken）。
  while (poolValues.has(candidate) || taken.has(candidate)) {
    n += 1;
    candidate = `claude-${tier}-${n}`;
  }
  return candidate;
}

/** 供应商是否为聚合供应商。 */
export function isAggregateProvider(provider: Pick<Provider, "meta">): boolean {
  return Boolean(provider.meta?.aggregateRoutes);
}

/** 对齐同版本后端的 Claude-safe ID 判定；保存校验仍以后端为权威。 */
function isCompatibleRouteId(id: string): boolean {
  const value = id.trim().toLowerCase();
  const vendor =
    /ark-code|astron|command-r|deepseek|doubao|gemini|gemma|glm|gpt|grok|hermes|hy3|kimi|lfm|\bling\b|llama|longcat|mimo|minimax|mistral|mixtral|moonshot|nemotron|openai|phi-|qianfan|qwen|tc-code|\bunic\b|yi-|stepfun|step-3|seed-|bytedance|hunyuan|granite|amazon\.nova|nova-|devstral|ministral|ernie|codex|arcee|trinity|abab|phi\d|\bk2\.|\bm2\.|jamba|arctic|solar|mercury|zamba|kat-coder|\bds-|dpsk/;
  return (
    !value.includes("[1m]") &&
    !vendor.test(value) &&
    /^(?:anthropic\/)?claude-(?:sonnet|opus|haiku|fable)-.+$/.test(value)
  );
}

export function needsRouteIdMigration(routes: AggregateRoutes): boolean {
  const seen = new Set<string>();
  return routes.slots.some((slot) => {
    const id = slot.routeId.trim().toLowerCase();
    const invalid = !isCompatibleRouteId(id) || seen.has(id);
    seen.add(id);
    return invalid;
  });
}

/** 仅供用户明确预览/确认迁移调用；普通编辑绝不自动转换旧 ID。 */
export function migrateIncompatibleRouteIds(
  routes: AggregateRoutes,
): AggregateRoutes {
  const counts = new Map<string, number>();
  for (const slot of routes.slots) {
    const id = slot.routeId.trim().toLowerCase();
    counts.set(id, (counts.get(id) ?? 0) + 1);
  }
  const slots = routes.slots.map((slot) => {
    const id = slot.routeId.trim().toLowerCase();
    const replace = !isCompatibleRouteId(id) || (counts.get(id) ?? 0) > 1;
    return replace ? { ...slot, routeId: "" } : slot;
  });
  const assigned = assignSlotIds({ ...routes, slots }, routes);
  const remap = new Map<string, string>();
  routes.slots.forEach((slot, i) => {
    if (slot.routeId && !remap.has(slot.routeId))
      remap.set(slot.routeId, assigned.slots[i].routeId);
  });
  return {
    ...assigned,
    slots: assigned.slots.map((slot) =>
      slot.maxEffort && !effortLevelsFor(slot.routeId).includes(slot.maxEffort)
        ? { ...slot, maxEffort: undefined }
        : slot,
    ),
    defaultTarget:
      routes.defaultTarget.kind === "slotId"
        ? {
            kind: "slotId",
            value:
              remap.get(routes.defaultTarget.value) ??
              routes.defaultTarget.value,
          }
        : routes.defaultTarget,
    defaultModel: routes.defaultModel
      ? remap.get(routes.defaultModel)
      : undefined,
    ...(routes.aliasRules
      ? {
          aliasRules: routes.aliasRules.map((rule) => ({
            ...rule,
            slotId: remap.get(rule.slotId) ?? "",
          })),
        }
      : {}),
  };
}

/** 由槽位派生标签（选择器显示名）：显式填写优先，否则「供应商 · 上游模型」。
 *
 *  未填写时**必须**给出非空默认值——空值会让 profile 省略 `labelOverride`，
 *  Claude Desktop 便退回「按 ID 自动格式化」，选择器里只剩 `claude-opus-2[1m]`
 *  这类看不出实际调用哪家模型的名字。 */
export function slotLabel(
  slot: {
    upstreamModel: string;
    label?: string;
  },
  providerName?: string,
): string {
  const explicit = slot.label?.trim();
  if (explicit && explicit.length > 0) return explicit;
  const model = slot.upstreamModel.trim();
  const name = providerName?.trim();
  if (!name) return model;
  return model ? `${name} · ${model}` : name;
}

/** 路由表是否可保存（至少一个槽位，且默认目标已选定非空值）。 */
export function canSaveAggregateRoutes(
  routes: AggregateRoutes | undefined | null,
): boolean {
  return Boolean(
    routes &&
      routes.slots.length > 0 &&
      routes.defaultTarget?.value.trim() &&
      (routes.defaultTarget.kind !== "slotId" ||
        routes.slots.some(
          (s) => s.routeId.trim() === routes.defaultTarget.value.trim(),
        )),
  );
}

/**
 * 供应商卡内档位行的固定顺序（从强到弱）。槽位在 `slots[]` 里的持久化顺序
 * 就是按这个序展平的（见 `flattenProviderGroups`），保证 ID 分配确定。
 */
export const TIER_ROW_ORDER: readonly AggregateTier[] = [
  "fable",
  "opus",
  "sonnet",
  "haiku",
];

/** 供应商卡的视图形状：一家供应商 + 各档位已映射的槽位（未映射的档位无键）。 */
export interface ProviderTierRows {
  providerId: string;
  rows: Partial<Record<AggregateTier, AggregateRouteSlot[]>>;
}

/**
 * 扁平槽位按供应商分组（编辑器渲染用）。卡序 = 供应商在列表里的首次出现顺序。
 * 同供应商同档位可以有多个模型，按原出现顺序完整保留。
 */
export function groupSlotsByProvider(
  slots: AggregateRouteSlot[],
): ProviderTierRows[] {
  const cards: ProviderTierRows[] = [];
  const byProvider = new Map<string, ProviderTierRows>();
  for (const slot of slots) {
    let card = byProvider.get(slot.providerId);
    if (!card) {
      card = { providerId: slot.providerId, rows: {} };
      byProvider.set(slot.providerId, card);
      cards.push(card);
    }
    (card.rows[slot.tier] ??= []).push(slot);
  }
  return cards;
}

/** 卡序 × 档位序（`TIER_ROW_ORDER`）重建扁平槽位数组，未映射档位不产出。 */
export function flattenProviderGroups(
  cards: ProviderTierRows[],
): AggregateRouteSlot[] {
  return cards.flatMap((card) => {
    const rows: AggregateRouteSlot[] = [];
    for (const tier of TIER_ROW_ORDER) {
      rows.push(...(card.rows[tier] ?? []));
    }
    return rows;
  });
}

/** 只给新槽位分配 ID；既有身份不随编辑或排序改变。删除记录跨保存保留。 */
export function assignSlotIds(
  routes: AggregateRoutes,
  previous?: AggregateRoutes,
): AggregateRoutes {
  const activeIds = new Set(routes.slots.map((s) => s.routeId).filter(Boolean));
  const retired = new Set([
    ...(previous?.retiredRouteIds ?? []),
    ...(routes.retiredRouteIds ?? []),
  ]);
  for (const slot of previous?.slots ?? []) {
    if (slot.routeId && !activeIds.has(slot.routeId)) retired.add(slot.routeId);
  }
  const canonical = (id: string) => id.trim().toLowerCase();
  const taken = new Set([...activeIds, ...retired].map(canonical));
  // 悬空引用也不得被一条新行悄悄认领；默认目标留给保存校验要求重新选择。
  if (routes.defaultTarget.kind === "slotId" && routes.defaultTarget.value)
    taken.add(canonical(routes.defaultTarget.value));
  if (routes.defaultModel) taken.add(canonical(routes.defaultModel));
  for (const rule of routes.aliasRules ?? [])
    if (rule.slotId) taken.add(canonical(rule.slotId));
  const slots = routes.slots.map((slot) => {
    if (slot.routeId) return { ...slot };
    let ordinal = 1;
    let id = slotId(slot.tier, ordinal, taken);
    while (taken.has(canonical(id))) id = slotId(slot.tier, ++ordinal, taken);
    taken.add(canonical(id));
    return { ...slot, routeId: id };
  });
  const live = new Set(slots.map((s) => s.routeId.trim()));
  const defaultModel =
    routes.defaultModel && live.has(routes.defaultModel.trim())
      ? routes.defaultModel
      : undefined;
  const aliasRules = routes.aliasRules?.map((rule) => ({
    ...rule,
    slotId: rule.slotId && live.has(rule.slotId.trim()) ? rule.slotId : "",
  }));
  return {
    ...routes,
    slots,
    defaultModel,
    ...(aliasRules ? { aliasRules } : {}),
    ...(retired.size ? { retiredRouteIds: [...retired] } : {}),
  };
}
