import type { AggregateRoutes, AggregateTier, Provider } from "@/types";

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
 * 池内顺序按「强度档位是否齐全」排：`claude-opus-4-8`/`4-7` 有 low…max（含 xhigh），
 * `4-6` 只有 extended 开关；不取 `claude-opus-5`（它 `disallowThinkingDisabled`，
 * 会强制开启思考）。池子用尽后退回 `claude-{档位}-{序号}`：形状合法、可路由，
 * 只是那条不再有强度控件。
 *
 * **池内 ID 不得形如 `claude-{档位}-{数字}`**，否则会与溢出生成值撞名
 * （`claude-sonnet-5` 就踩过：同档第 5 个槽位也生成 `claude-sonnet-5`，两条槽位同 ID，
 * 后端去重会吃掉一条）。故 sonnet 池从 `4-6` 起，不取 `claude-sonnet-5`
 * ——两者的强度阶梯本就一样。
 */
const RECOGNIZED_IDS: Record<AggregateTier, readonly string[]> = {
  opus: ["claude-opus-4-8", "claude-opus-4-7", "claude-opus-4-6"],
  sonnet: ["claude-sonnet-4-6", "claude-sonnet-4-5"],
  haiku: ["claude-haiku-4-5"],
  // fable 族的强度阶梯由族正则兜底，序号可无限生成，无需 ID 池。
  fable: [],
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
export function slotId(tier: AggregateTier, ordinal: number): string {
  return RECOGNIZED_IDS[tier][ordinal - 1] ?? `claude-${tier}-${ordinal}`;
}

/** 供应商是否为聚合供应商。 */
export function isAggregateProvider(provider: Pick<Provider, "meta">): boolean {
  return Boolean(provider.meta?.aggregateRoutes);
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
    routes && routes.slots.length > 0 && routes.defaultTarget?.value.trim(),
  );
}

/** 为所有槽位重新生成 routeId（按档位分别编号），并让默认目标按位置跟随。
 *
 *  在槽位增删、档位变更后调用，也可用于**迁移存量 ID**（旧方案含供应商名，
 *  会被 Claude Desktop 整组拒绝）。默认目标若引用的是槽位 ID：以它在**本表内
 *  的位置**取新 ID；目标槽位已被删除（旧 ID 不在本表里）时保持原值，交由保存
 *  校验拦截，绝不静默改指到另一个槽位。 */
export function assignSlotIds(routes: AggregateRoutes): AggregateRoutes {
  const ordinalByTier = new Map<AggregateTier, number>();
  const ids = routes.slots.map((slot) => {
    const ordinal = (ordinalByTier.get(slot.tier) ?? 0) + 1;
    ordinalByTier.set(slot.tier, ordinal);
    return slotId(slot.tier, ordinal);
  });

  const slots = routes.slots.map((slot, index) => ({
    ...slot,
    routeId: ids[index],
  }));

  let defaultTarget = routes.defaultTarget;
  if (defaultTarget.kind === "slotId") {
    const index = routes.slots.findIndex(
      (slot) => slot.routeId === defaultTarget.value,
    );
    if (index >= 0) {
      defaultTarget = { kind: "slotId", value: ids[index] };
    }
  }

  return { ...routes, slots, defaultTarget };
}
