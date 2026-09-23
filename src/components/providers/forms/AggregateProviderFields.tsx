import { useMemo, useState } from "react";
import { Download, Loader2, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ImeSafeInput } from "@/components/ui/ime-safe-input";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ModelDropdown } from "./shared/ModelDropdown";
import type { FetchedModel } from "@/lib/api/model-fetch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useTranslation } from "react-i18next";
import type {
  AggregateRouteSlot,
  AggregateRoutes,
  AggregateTier,
  DefaultTarget,
  Provider,
} from "@/types";
import {
  assignSlotIds,
  canSaveAggregateRoutes,
  flattenProviderGroups,
  groupSlotsByProvider,
  slotLabel,
  TIER_ROW_ORDER,
  type ProviderTierRows,
} from "@/utils/aggregateRoutes";

interface Props {
  value: AggregateRoutes;
  onChange: (next: AggregateRoutes) => void;
  /** 可作目标的常规供应商（不含聚合供应商自身与官方供应商，防止嵌套/无凭据目标） */
  candidates: Provider[];
  /** 该目标供应商已拉取到的模型列表；按供应商 id 缓存，同卡各档位共用 */
  modelsForProvider: (providerId: string) => FetchedModel[];
  /** 正在拉取的供应商 id；非 null 时所有拉取按钮进入 loading */
  fetchingProviderId: string | null;
  onFetchModels: (provider: Provider) => void;
}

/** 深拷贝卡片数组（行对象只读不改、整体替换，浅拷贝两层即可）。 */
const cloneCards = (cards: ProviderTierRows[]): ProviderTierRows[] =>
  cards.map((card) => ({
    providerId: card.providerId,
    rows: { ...card.rows },
  }));

export function AggregateProviderFields({
  value,
  onChange,
  candidates,
  modelsForProvider,
  fetchingProviderId,
  onFetchModels,
}: Props) {
  const { t } = useTranslation();

  // 显示名默认值要带上供应商名：同一个上游模型可能挂在两家供应商上，
  // 只写模型名仍分不清实际调用的是哪一家。
  const providerNameById = new Map(candidates.map((p) => [p.id, p.name]));
  const labelOf = (slot: AggregateRouteSlot) =>
    slotLabel(slot, providerNameById.get(slot.providerId));
  const nameOf = (id: string) => providerNameById.get(id) ?? id;

  // 编辑器按「供应商卡」渲染，存储仍是扁平 slots[]（分组是纯视图概念，
  // 后端与持久化零改动）。供应商被选了但还没映射任何档位时，slots 里没有
  // 它的痕迹，用 pendingProviders 记住这张空卡，否则它会在重渲染时消失。
  const [pendingProviders, setPendingProviders] = useState<string[]>([]);

  const grouped = useMemo(
    () => groupSlotsByProvider(value.slots),
    [value.slots],
  );
  const cards = useMemo(() => {
    const groupedIds = new Set(grouped.map((card) => card.providerId));
    const extras = pendingProviders
      .filter((id) => !groupedIds.has(id))
      .map((id): ProviderTierRows => ({ providerId: id, rows: {} }));
    return [...grouped, ...extras];
  }, [grouped, pendingProviders]);

  // 每次变更都重算所有槽位 ID：保证「预览 = 实际提交值」。
  // assignSlotIds 会按档位从零重编号，并让引用槽位 ID 的默认目标按位置跟随，
  // 故增删槽位后引用不会悬空（目标槽位被删则保持原值，交由保存校验拦截）。
  const commit = (next: AggregateRoutes) => onChange(assignSlotIds(next));

  /** 提交编辑后的卡片列表：有行的卡展平回 slots，空卡（选了供应商还没映射
   *  任何档位）存进 pendingProviders。 */
  const commitCards = (next: ProviderTierRows[]) => {
    const hasRows = (card: ProviderTierRows) =>
      TIER_ROW_ORDER.some((tier) => card.rows[tier]);
    setPendingProviders(
      next.filter((card) => !hasRows(card)).map((c) => c.providerId),
    );
    commit({
      ...value,
      slots: flattenProviderGroups(next.filter(hasRows)),
    });
  };

  const setCardProvider = (cardIndex: number, providerId: string) => {
    const next = cloneCards(cards);
    const card = next[cardIndex];
    card.providerId = providerId;
    // 行内的槽位各自带着 providerId（扁平存储的冗余），换卡头时同步改写
    for (const tier of TIER_ROW_ORDER) {
      const slot = card.rows[tier];
      if (slot) card.rows[tier] = { ...slot, providerId };
    }
    commitCards(next);
  };

  /** 上游模型输入即该档槽位的生死线：非空 = 映射，清空 = 移除（「映射几档
   *  就有几个」）。新槽位 routeId 留空，由 commit 里的 assignSlotIds 统一编号。 */
  const setUpstream = (cardIndex: number, tier: AggregateTier, raw: string) => {
    const next = cloneCards(cards);
    const card = next[cardIndex];
    const existing = card.rows[tier];
    if (raw.trim() === "") {
      delete card.rows[tier];
    } else if (existing) {
      card.rows[tier] = { ...existing, upstreamModel: raw };
    } else {
      card.rows[tier] = {
        routeId: "",
        tier,
        providerId: card.providerId,
        upstreamModel: raw,
        supports1m: false,
      };
    }
    commitCards(next);
  };

  const patchRow = (
    cardIndex: number,
    tier: AggregateTier,
    patch: Partial<AggregateRouteSlot>,
  ) => {
    const next = cloneCards(cards);
    const slot = next[cardIndex].rows[tier];
    if (!slot) return;
    next[cardIndex].rows[tier] = { ...slot, ...patch };
    commitCards(next);
  };

  const removeCard = (cardIndex: number) =>
    commitCards(cards.filter((_, i) => i !== cardIndex));

  const addCard = () => {
    const used = new Set(cards.map((card) => card.providerId));
    const free = candidates.find((p) => !used.has(p.id));
    if (!free) return;
    commitCards([...cards, { providerId: free.id, rows: {} }]);
  };

  return (
    <section className="space-y-4">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">{t("aggregate.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("aggregate.hint")}</p>
      </header>

      {!canSaveAggregateRoutes(value) && (
        <p className="text-xs text-destructive">{t("aggregate.notSaveable")}</p>
      )}

      {/* 默认目标是必填项，放在供应商卡**之前**：原先它在槽位列表下方，列表一多就被
          顶出视野、又渲染成一个没有占位文案的空白下拉框，用户会卡在这里（2026-09-22
          实测）。它的选项来自供应商与槽位，槽位为空时先选一家供应商也成立。 */}
      <div className="space-y-1">
        <Label className="text-xs">{t("aggregate.defaultTarget")}</Label>
        <Select
          value={
            value.defaultTarget.kind === "providerId"
              ? `provider:${value.defaultTarget.value}`
              : `slot:${value.defaultTarget.value}`
          }
          onValueChange={(v) => {
            const [kind, ...rest] = v.split(":");
            const target: DefaultTarget =
              kind === "provider"
                ? { kind: "providerId", value: rest.join(":") }
                : { kind: "slotId", value: rest.join(":") };
            onChange({ ...value, defaultTarget: target });
          }}
        >
          <SelectTrigger className="h-8">
            <SelectValue
              placeholder={t("aggregate.defaultTargetPlaceholder", {
                defaultValue: "必选：未命中槽位的请求回落到这里",
              })}
            />
          </SelectTrigger>
          <SelectContent>
            {candidates.map((p) => (
              <SelectItem key={p.id} value={`provider:${p.id}`}>
                {p.name}
              </SelectItem>
            ))}
            {value.slots.map((s, index) => (
              <SelectItem
                key={s.routeId || `slot-${index}`}
                value={`slot:${s.routeId}`}
              >
                {labelOf(s)} ({s.routeId})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">
          {t("aggregate.defaultTargetHint")}
        </p>
      </div>

      <div className="space-y-3">
        {/* 列头（对齐「模型映射」表格的样式，2026-09-23 用户反馈）。
            所有卡共用同一套列，放在卡列表上方一次即可。 */}
        {cards.length > 0 && (
          <div className="flex items-center gap-2 px-3 text-xs text-muted-foreground">
            <span className="w-14 shrink-0">{t("aggregate.tier")}</span>
            <span className="min-w-0 flex-1">
              {t("aggregate.upstreamModel")}
            </span>
            <span className="min-w-0 flex-1">{t("aggregate.displayName")}</span>
            <span className="shrink-0">{t("aggregate.supports1m")}</span>
          </div>
        )}
        {cards.map((card, cardIndex) => {
          const usedElsewhere = new Set(
            cards.filter((_, i) => i !== cardIndex).map((c) => c.providerId),
          );
          const selectable = candidates.filter((p) => !usedElsewhere.has(p.id));
          const target = candidates.find((p) => p.id === card.providerId);
          const fetched = modelsForProvider(card.providerId);
          return (
            <div
              key={card.providerId || `card-${cardIndex}`}
              className="space-y-2 rounded-md border border-border-default p-3"
            >
              {/* 卡头：一家供应商一张卡。模型列表按供应商拉一次，卡内各档位共用缓存。 */}
              <div className="flex items-center gap-2">
                <Select
                  value={card.providerId}
                  onValueChange={(v) => setCardProvider(cardIndex, v)}
                >
                  <SelectTrigger className="h-8 flex-1">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {selectable.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                    {/* 存量数据可能引用已被删除/被其他卡占用的供应商：
                        保底显示，避免选择器空白让用户误以为没配置 */}
                    {!selectable.some((p) => p.id === card.providerId) && (
                      <SelectItem value={card.providerId}>
                        {nameOf(card.providerId)}
                      </SelectItem>
                    )}
                  </SelectContent>
                </Select>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-8 shrink-0 gap-1"
                  disabled={!target || fetchingProviderId !== null}
                  onClick={() => target && onFetchModels(target)}
                >
                  {fetchingProviderId === card.providerId ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Download className="h-3.5 w-3.5" />
                  )}
                  {t("providerForm.fetchModels", {
                    defaultValue: "获取模型列表",
                  })}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 shrink-0"
                  onClick={() => removeCard(cardIndex)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>

              {/* 固定四档行（从强到弱）。上游模型非空即映射该档，清空即移除；
                  三列等高平行——旧版按钮挤在「上游模型」标签行里导致两列输入框
                  错位（2026-09-23 用户反馈），按钮上移卡头后不复存在。 */}
              {TIER_ROW_ORDER.map((tier) => {
                const slot = card.rows[tier];
                const mapped = Boolean(slot?.upstreamModel.trim());
                const switchId = `agg-1m-${cardIndex}-${tier}`;
                return (
                  <div
                    key={tier}
                    className="flex items-center gap-2"
                    // 槽位 ID 不占版面（2026-09-23 用户反馈：纯内部代号没必要常显），
                    // 悬停可查——排查代理日志里的 request_model 时用得上
                    title={slot?.routeId || undefined}
                  >
                    <span className="w-14 shrink-0 text-xs text-muted-foreground">
                      {tier}
                    </span>
                    <div className="flex min-w-0 flex-1 gap-1">
                      <Input
                        className="h-8 min-w-0 flex-1"
                        value={slot?.upstreamModel ?? ""}
                        placeholder={t("aggregate.upstreamModelPlaceholder", {
                          defaultValue: "留空 = 不映射该档",
                        })}
                        onChange={(e) =>
                          setUpstream(cardIndex, tier, e.target.value)
                        }
                      />
                      {fetched.length > 0 && (
                        <ModelDropdown
                          models={fetched}
                          onSelect={(id) => setUpstream(cardIndex, tier, id)}
                        />
                      )}
                    </div>
                    <ImeSafeInput
                      className="h-8 min-w-0 flex-1"
                      disabled={!mapped}
                      placeholder={
                        slot ? labelOf(slot) : t("aggregate.displayName")
                      }
                      value={slot?.label ?? ""}
                      // 留空则不落库；提交时由表单统一补成「供应商 · 上游模型」
                      // （见 ClaudeDesktopProviderForm 的提交处），故这里存空串即可。
                      onValueChange={(v) =>
                        patchRow(cardIndex, tier, {
                          label: v.trim() ? v : undefined,
                        })
                      }
                    />
                    <div className="flex shrink-0 items-center gap-1.5">
                      <Switch
                        id={switchId}
                        disabled={!mapped}
                        checked={Boolean(slot?.supports1m)}
                        onCheckedChange={(v) =>
                          patchRow(cardIndex, tier, { supports1m: v })
                        }
                      />
                      <Label htmlFor={switchId} className="text-xs">
                        {t("aggregate.supports1m")}
                      </Label>
                    </div>
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>

      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 gap-1.5"
        disabled={
          candidates.length > 0 &&
          new Set(cards.map((card) => card.providerId)).size >=
            candidates.length
        }
        onClick={addCard}
      >
        <Plus className="h-3.5 w-3.5" />
        {t("aggregate.addProvider")}
      </Button>
    </section>
  );
}
