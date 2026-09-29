import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Download,
  GripVertical,
  Loader2,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { CSSProperties, ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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

const UNSET = "__unset__";

/** 卡片可拖拽包装：把手 props 通过 render-prop 交给卡头。 */
function SortableCard({
  id,
  children,
}: {
  id: string;
  children: (handle: ReactNode) => ReactNode;
}) {
  const { setNodeRef, attributes, listeners, transform, transition } =
    useSortable({ id });
  const style: CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
  };
  return (
    <div ref={setNodeRef} style={style}>
      {children(
        <button
          type="button"
          className="shrink-0 cursor-grab touch-none text-muted-foreground/60 hover:text-foreground"
          aria-label="drag"
          {...attributes}
          {...listeners}
        >
          <GripVertical className="h-4 w-4" />
        </button>,
      )}
    </div>
  );
}

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

  // 折叠状态：expandedIds 里的卡展开，其余（含新卡）默认折叠。
  // 用「展开集合」而非「折叠集合」：新增卡无需手动登记即默认折叠。
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

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
  // assignSlotIds 会按档位从零重编号，并让默认目标/默认模型按位置跟随，
  // 故增删槽位后引用不会悬空（默认目标悬空保持原值交由保存校验拦截；
  // 默认模型悬空置空回落排序首位）。
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

  const removeRow = (cardIndex: number, tier: AggregateTier) => {
    const next = cloneCards(cards);
    delete next[cardIndex].rows[tier];
    commitCards(next);
  };

  /** 新增档位行：只允许未占用的档（v3：增量添加替代固定四行）。 */
  const addRow = (cardIndex: number, tier: AggregateTier) => {
    const next = cloneCards(cards);
    next[cardIndex].rows[tier] = {
      routeId: "",
      tier,
      providerId: next[cardIndex].providerId,
      upstreamModel: "",
      supports1m: false,
    };
    // 新行自动展开（用户要立刻填它）
    const cardId = next[cardIndex].providerId;
    setExpandedIds((prev) => new Set(prev).add(cardId));
    commitCards(next);
  };

  const removeCard = (cardIndex: number) =>
    commitCards(cards.filter((_, i) => i !== cardIndex));

  const addCard = () => {
    const used = new Set(cards.map((card) => card.providerId));
    const free = candidates.find((p) => !used.has(p.id));
    if (!free) return;
    setExpandedIds((prev) => new Set(prev).add(free.id)); // 新卡自动展开
    commitCards([...cards, { providerId: free.id, rows: {} }]);
  };

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const from = cards.findIndex((card) => card.providerId === active.id);
    const to = cards.findIndex((card) => card.providerId === over.id);
    if (from < 0 || to < 0) return;
    commitCards(arrayMove(cards, from, to));
  };

  /** 已映射（上游模型非空）的槽位——默认模型下拉与「生效默认」判定都只认它们。 */
  const mappedSlots = useMemo(
    () => value.slots.filter((slot) => slot.upstreamModel.trim() !== ""),
    [value.slots],
  );
  // 生效默认：显式设置且仍指向已映射槽 → 用它；否则排序首位
  const effectiveDefault =
    value.defaultModel &&
    mappedSlots.some((slot) => slot.routeId === value.defaultModel)
      ? value.defaultModel
      : (mappedSlots[0]?.routeId ?? "");

  const toggleCard = (id: string) =>
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });

  const allExpanded =
    cards.length > 0 && cards.every((card) => expandedIds.has(card.providerId));

  return (
    <section className="space-y-4">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">{t("aggregate.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("aggregate.hint")}</p>
      </header>

      {!canSaveAggregateRoutes(value) && (
        <p className="text-xs text-destructive">{t("aggregate.notSaveable")}</p>
      )}

      {/* 兜底目标与默认模型并排（v3）。兜底目标是必填的路由安全网；默认模型
          决定 Claude Desktop 新会话的起点（写 profile 时置顶到第一条）。 */}
      <div className="grid grid-cols-2 gap-3">
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

        <div className="space-y-1">
          <Label className="text-xs">{t("aggregate.defaultModel")}</Label>
          <Select
            value={value.defaultModel ?? UNSET}
            onValueChange={(v) =>
              onChange({
                ...value,
                defaultModel: v === UNSET ? undefined : v,
              })
            }
          >
            <SelectTrigger className="h-8">
              <SelectValue
                placeholder={t("aggregate.defaultModelPlaceholder", {
                  defaultValue: "未设置：跟随排序首位",
                })}
              />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={UNSET}>
                {t("aggregate.defaultModelPlaceholder", {
                  defaultValue: "未设置：跟随排序首位",
                })}
              </SelectItem>
              {mappedSlots.map((slot) => (
                <SelectItem key={slot.routeId} value={slot.routeId}>
                  {labelOf(slot)} ({slot.routeId})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {t("aggregate.defaultModelHint", {
              defaultValue: "启动新会话时使用的模型；未设置时取排序第一条",
            })}
          </p>
        </div>
      </div>

      {/* 全部展开 / 全部折叠（折叠状态不持久化，每次打开表单重置为全折叠） */}
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-xs"
          disabled={allExpanded}
          onClick={() =>
            setExpandedIds(new Set(cards.map((card) => card.providerId)))
          }
        >
          {t("aggregate.expandAll", { defaultValue: "全部展开" })}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-xs"
          disabled={expandedIds.size === 0}
          onClick={() => setExpandedIds(new Set())}
        >
          {t("aggregate.collapseAll", { defaultValue: "全部折叠" })}
        </Button>
      </div>

      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={handleDragEnd}
      >
        <SortableContext
          items={cards.map((card) => card.providerId)}
          strategy={verticalListSortingStrategy}
        >
          <div className="space-y-3">
            {cards.map((card, cardIndex) => {
              const usedElsewhere = new Set(
                cards
                  .filter((_, i) => i !== cardIndex)
                  .map((c) => c.providerId),
              );
              const selectable = candidates.filter(
                (p) => !usedElsewhere.has(p.id),
              );
              const target = candidates.find((p) => p.id === card.providerId);
              const fetched = modelsForProvider(card.providerId);
              const mappedTiers = TIER_ROW_ORDER.filter(
                (tier) => card.rows[tier],
              );
              const unusedTiers = TIER_ROW_ORDER.filter(
                (tier) => !card.rows[tier],
              );
              const expanded = expandedIds.has(card.providerId);
              // routeId 恒非空（每次 commit 都经 assignSlotIds 赋真 ID），无已映射
              // 槽位时 effectiveDefault 为 ""，恒不命中，无需再挡空值。
              const isDefaultCard = Object.values(card.rows).some(
                (slot) => slot?.routeId === effectiveDefault,
              );
              return (
                <SortableCard key={card.providerId} id={card.providerId}>
                  {(handle) => (
                    <div className="space-y-2 rounded-md border border-border-default p-3">
                      {/* 卡头：拖拽把手 + 供应商 + 拉模型 + 删卡 + 折叠开关。
                          模型列表按供应商拉一次，卡内各档位共用缓存。 */}
                      <div className="flex items-center gap-2">
                        {handle}
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
                            {!selectable.some(
                              (p) => p.id === card.providerId,
                            ) && (
                              <SelectItem value={card.providerId}>
                                {nameOf(card.providerId)}
                              </SelectItem>
                            )}
                          </SelectContent>
                        </Select>
                        {isDefaultCard && (
                          <Badge
                            variant="secondary"
                            className="shrink-0 text-[10px]"
                          >
                            {t("aggregate.defaultBadge", {
                              defaultValue: "默认",
                            })}
                          </Badge>
                        )}
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
                        <button
                          type="button"
                          className="shrink-0 text-muted-foreground hover:text-foreground"
                          aria-expanded={expanded}
                          onClick={() => toggleCard(card.providerId)}
                        >
                          {expanded ? (
                            <ChevronDown className="h-4 w-4" />
                          ) : (
                            <ChevronRight className="h-4 w-4" />
                          )}
                        </button>
                      </div>

                      {expanded ? (
                        <>
                          {/* 列头：紧贴供应商行（2026-09-23 用户确认的草图） */}
                          <div className="flex items-center gap-2 text-xs text-muted-foreground">
                            <span className="w-14 shrink-0">
                              {t("aggregate.tier")}
                            </span>
                            <span className="min-w-0 flex-1">
                              {t("aggregate.upstreamModel")}
                            </span>
                            <span className="min-w-0 flex-1">
                              {t("aggregate.displayName")}
                            </span>
                            <span className="w-20 shrink-0 text-right">
                              {t("aggregate.supports1m")}
                            </span>
                            <span className="w-6 shrink-0" />
                          </div>

                          {/* 已映射档位的行（v3：增量添加，不再固定四行）。行尾 ×
                              删除该槽；行内不支持改档位（删了重加）。 */}
                          {mappedTiers.map((tier) => {
                            const slot = card.rows[tier];
                            if (!slot) return null;
                            const isEffectiveDefault =
                              slot.routeId === effectiveDefault;
                            const switchId = `agg-1m-${cardIndex}-${tier}`;
                            return (
                              <div
                                key={tier}
                                className="flex items-center gap-2"
                                // 槽位 ID 不占版面，悬停可查——排查代理日志里的
                                // request_model 时用得上
                                title={slot.routeId || undefined}
                              >
                                <span className="w-14 shrink-0 text-xs text-muted-foreground">
                                  {tier}
                                </span>
                                <div className="flex min-w-0 flex-1 gap-1">
                                {/* 上游模型只改值，不再承担行的生死——删行走
                                    行尾 × 按钮（v3 语义）。 */}
                                  <Input
                                    className="h-8 min-w-0 flex-1"
                                    value={slot.upstreamModel}
                                    placeholder={t(
                                      "aggregate.upstreamModelPlaceholder",
                                      { defaultValue: "上游模型名" },
                                    )}
                                    onChange={(e) =>
                                      patchRow(cardIndex, tier, {
                                        upstreamModel: e.target.value,
                                      })
                                    }
                                  />
                                  {fetched.length > 0 && (
                                    <ModelDropdown
                                      models={fetched}
                                      onSelect={(id) =>
                                        patchRow(cardIndex, tier, {
                                          upstreamModel: id,
                                        })
                                      }
                                    />
                                  )}
                                </div>
                                <div className="relative min-w-0 flex-1">
                                  <ImeSafeInput
                                    className="h-8 w-full pr-14"
                                    placeholder={labelOf(slot)}
                                    value={slot.label ?? ""}
                                    // 留空则不落库；提交时由表单统一补成「供应商 · 上游模型」
                                    onValueChange={(v) =>
                                      patchRow(cardIndex, tier, {
                                        label: v.trim() ? v : undefined,
                                      })
                                    }
                                  />
                                  {isEffectiveDefault && (
                                    <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-muted-foreground">
                                      {t("aggregate.defaultBadge", {
                                        defaultValue: "默认",
                                      })}
                                    </span>
                                  )}
                                </div>
                                <div className="flex w-20 shrink-0 items-center justify-end gap-1.5">
                                  <Switch
                                    id={switchId}
                                    checked={Boolean(slot.supports1m)}
                                    onCheckedChange={(v) =>
                                      patchRow(cardIndex, tier, {
                                        supports1m: v,
                                      })
                                    }
                                  />
                                  <Label
                                    htmlFor={switchId}
                                    className="text-xs text-muted-foreground"
                                  >
                                    1M
                                  </Label>
                                </div>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="icon"
                                  className="h-7 w-6 shrink-0 text-muted-foreground hover:text-destructive"
                                  title={t("aggregate.removeModel", {
                                    defaultValue: "删除该模型",
                                  })}
                                  onClick={() => removeRow(cardIndex, tier)}
                                >
                                  <X className="h-3.5 w-3.5" />
                                </Button>
                              </div>
                            );
                          })}

                          {/* 新增模型：只列未占用的档位；满 4 档隐藏 */}
                          {unusedTiers.length > 0 && (
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="sm"
                                  className="h-7 gap-1 text-xs text-muted-foreground"
                                >
                                  <Plus className="h-3.5 w-3.5" />
                                  {t("aggregate.addModel", {
                                    defaultValue: "新增模型",
                                  })}
                                </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="start">
                                {unusedTiers.map((tier) => (
                                  <DropdownMenuItem
                                    key={tier}
                                    onClick={() => addRow(cardIndex, tier)}
                                  >
                                    {tier}
                                  </DropdownMenuItem>
                                ))}
                              </DropdownMenuContent>
                            </DropdownMenu>
                          )}
                        </>
                      ) : (
                        /* 折叠态摘要：N 个模型 */
                        <p className="text-xs text-muted-foreground">
                          {t("aggregate.modelsSummary", {
                            count: mappedTiers.length,
                            defaultValue: "{{count}} 个模型",
                          })}
                        </p>
                      )}
                    </div>
                  )}
                </SortableCard>
              );
            })}
          </div>
        </SortableContext>
      </DndContext>

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
