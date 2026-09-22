import type { AggregateRoutes, AggregateTier, Provider } from "@/types";

/**
 * 把供应商名规范为路由段 slug，产出仅含 [a-z0-9-]，且无首尾 '-'。
 *
 * 这是槽位 ID 形状的唯一权威实现：后端不再有对应的 slugify（它只对自己的
 * 结果做 is_claude_safe_model_id 校验），所以这里的算法与测试即规范。
 *
 * 流程：转小写保留字母数字 → 其他字符折叠为单个 '-' → 去首尾 '-' →
 * 截断 20 字符 → 再次去尾部 '-'（截断可能正好落在分隔符上）；结果为空
 * 时回落为 provider id 前 8 位。
 */
export function slugify(providerName: string, providerId: string): string {
  let out = "";
  let lastDash = false;
  for (const ch of providerName) {
    if (/[A-Za-z0-9]/.test(ch)) {
      out += ch.toLowerCase();
      lastDash = false;
    } else if (!lastDash && out.length > 0) {
      out += "-";
      lastDash = true;
    }
  }
  const trimmed = out.replace(/^-+|-+$/g, "");
  const truncated = trimmed.slice(0, 20).replace(/-+$/g, "");
  return truncated.length > 0 ? truncated : providerId.slice(0, 8);
}

/**
 * 生成 claude-{tier}-{slug}；若已被占用则追加 -2、-3……直到空闲。
 *
 * 注意：taken 里的每个候选都要检查，不能只看 base 本身——否则
 * taken=["claude-sonnet-glm","claude-sonnet-glm-2"] 时会重复返回 -2。
 */
export function generateSlotId(
  tier: AggregateTier,
  providerName: string,
  providerId: string,
  taken: string[],
): string {
  const base = `claude-${tier}-${slugify(providerName, providerId)}`;
  if (!taken.includes(base)) return base;
  let n = 2;
  for (;;) {
    const candidate = `${base}-${n}`;
    if (!taken.includes(candidate)) return candidate;
    n += 1;
  }
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

/** 为所有槽位重新生成 routeId（按当前顺序去重），返回新的路由表。
 *  在槽位增删、目标供应商变更、档位变更后调用——保证预览与实际提交值一致。 */
export function assignSlotIds(
  routes: AggregateRoutes,
  providers: Pick<Provider, "id" | "name">[],
): AggregateRoutes {
  const taken: string[] = [];
  const slots = routes.slots.map((slot) => {
    const provider = providers.find((p) => p.id === slot.providerId);
    const routeId = generateSlotId(
      slot.tier,
      provider?.name ?? slot.providerId,
      slot.providerId,
      taken,
    );
    taken.push(routeId);
    return { ...slot, routeId };
  });
  return { ...routes, slots };
}
