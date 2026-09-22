import type { AggregateRoutes, AggregateTier, Provider } from "@/types";

/**
 * 生成槽位 ID：`claude-{档位}-{同档序号}`。
 *
 * 这个字符串就是 Claude Desktop 模型列表里的 `inferenceModels[].name`，也是
 * 请求按模型分流的键，因此**必须过 Claude Desktop 的校验**。实测（2.2553.1.0）
 * 它的校验是「厂商词黑名单 + Anthropic 形状」：
 *
 * ```js
 * Vxe = /ark-code|…|deepseek|glm|gpt|gemini|grok|kimi|qwen|…/   // 厂商词
 * Go  = (name) => Vxe.test(name) ? false : Wo.test(name) || name.includes("claude")
 * ```
 *
 * 也就是说**名字里只要出现 deepseek/glm 这类厂商词就会被判为
 * "is not an Anthropic model" 并从列表移除，且是整组生效**——旧方案把供应商名
 * slug 进 ID（`claude-fable-deepseek`、`claude-fable-zhipu-glm`），四个槽位被删光、
 * 选择器变空。故 ID 只由档位与序号构成，可读性交给「显示名」（`labelOverride`，
 * 它不受该校验约束）。
 */
export function slotId(tier: AggregateTier, ordinal: number): string {
  return `claude-${tier}-${ordinal}`;
}

/** 供应商是否为聚合供应商。 */
export function isAggregateProvider(provider: Pick<Provider, "meta">): boolean {
  return Boolean(provider.meta?.aggregateRoutes);
}

/** 由槽位派生标签（选择器显示名），缺省用上游模型名。 */
export function slotLabel(slot: {
  upstreamModel: string;
  label?: string;
}): string {
  const l = slot.label?.trim();
  return l && l.length > 0 ? l : slot.upstreamModel;
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
