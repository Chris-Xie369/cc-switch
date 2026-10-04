import { useEffect, useMemo, useState } from "react";
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
  AggregateAliasRule,
  AggregateMaxEffort,
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
  needsRouteIdMigration,
  migrateIncompatibleRouteIds,
} from "@/utils/aggregateRoutes";
import {
  effortCapability,
  effortLevelsFor,
} from "@/utils/claudeDesktopCapability";

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

  const [migrationPreview, setMigrationPreview] = useState<AggregateRoutes>();
  const [migrationBackup, setMigrationBackup] = useState(false);
  useEffect(() => {
    setMigrationPreview(undefined);
    setMigrationBackup(false);
  }, [value]);
  const downloadMigrationBackup = () => {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = "aggregate-routes-before-migration.json";
    link.click();
    URL.revokeObjectURL(url);
    setMigrationBackup(true);
  };

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

  // 新行分配 ID；既有 ID 不因排序或编辑变化。
  // assignSlotIds 保留既有身份，并记录删除的 ID 防止重用，
  // 删除引用目标时默认目标保持原值，由保存校验要求重新选择；
  // 默认模型悬空置空回落排序首位。
  //
  // 先为新行分配 ID，再按每行的实际兼容阶梯校验上限；既有 ID 和有效上限不变。
  const commit = (next: AggregateRoutes) => {
    const assigned = assignSlotIds(next, value);
    return onChange({
      ...assigned,
      slots: assigned.slots.map((s) =>
        s.maxEffort && effortLevelsFor(s.routeId).includes(s.maxEffort)
          ? s
          : { ...s, maxEffort: undefined },
      ),
    });
  };

  /** 提交编辑后的卡片列表：有行的卡展平回 slots，空卡（选了供应商还没映射
   *  任何档位）存进 pendingProviders。 */
  const commitCards = (next: ProviderTierRows[]) => {
    const hasRows = (card: ProviderTierRows) =>
      TIER_ROW_ORDER.some((tier) => card.rows[tier]?.length);
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
      const slots = card.rows[tier];
      if (slots)
        card.rows[tier] = slots.map((slot) => ({ ...slot, providerId }));
    }
    commitCards(next);
  };

  const patchRow = (
    cardIndex: number,
    tier: AggregateTier,
    rowIndex: number,
    patch: Partial<AggregateRouteSlot>,
  ) => {
    const next = cloneCards(cards);
    const slots = next[cardIndex].rows[tier];
    if (!slots?.[rowIndex]) return;
    next[cardIndex].rows[tier] = slots.map((slot, i) =>
      i === rowIndex ? { ...slot, ...patch } : slot,
    );
    commitCards(next);
  };

  const removeRow = (
    cardIndex: number,
    tier: AggregateTier,
    rowIndex: number,
  ) => {
    const next = cloneCards(cards);
    const remaining = (next[cardIndex].rows[tier] ?? []).filter(
      (_, i) => i !== rowIndex,
    );
    if (remaining.length) next[cardIndex].rows[tier] = remaining;
    else delete next[cardIndex].rows[tier];
    commitCards(next);
  };

  /** 每个档位可以添加多个独立模型。 */
  const addRow = (cardIndex: number, tier: AggregateTier) => {
    const next = cloneCards(cards);
    next[cardIndex].rows[tier] = [
      ...(next[cardIndex].rows[tier] ?? []),
      {
        routeId: "",
        tier,
        providerId: next[cardIndex].providerId,
        upstreamModel: "",
        supports1m: false,
      },
    ];
    // 新行自动展开（用户要立刻填它）
    const cardId = next[cardIndex].providerId;
    setExpandedIds((prev) => new Set(prev).add(cardId));
    commitCards(next);
  };

  const removeCard = (cardIndex: number) =>
    commitCards(cards.filter((_, i) => i !== cardIndex));

  /** 别名规则改动不改变槽位身份；按实际 ID 校验有效上限。 */
  const patchAliasRule = (
    index: number,
    patch: Partial<AggregateAliasRule>,
  ) => {
    const next = [...(value.aliasRules ?? [])];
    next[index] = { ...next[index], ...patch };
    commit({ ...value, aliasRules: next });
  };
  const addAliasRule = () =>
    commit({
      ...value,
      aliasRules: [...(value.aliasRules ?? []), { prefix: "", slotId: "" }],
    });
  const removeAliasRule = (index: number) =>
    commit({
      ...value,
      aliasRules: (value.aliasRules ?? []).filter((_, i) => i !== index),
    });

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

      {needsRouteIdMigration(value) && (
        <div className="space-y-2 rounded-md border p-3 text-xs">
          <p>
            {t("aggregate.migrationHint", {
              defaultValue:
                "存量路由 ID 不兼容或重复。普通编辑保留原 ID；转换前请备份，旧会话可能需要重新选择模型。",
            })}
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              setMigrationPreview(migrateIncompatibleRouteIds(value));
              setMigrationBackup(false);
            }}
          >
            {t("aggregate.migrationPreview", {
              defaultValue: "预览旧路由转换",
            })}
          </Button>
          {migrationPreview && (
            <>
              <pre className="whitespace-pre-wrap">
                {value.slots
                  .map(
                    (slot, i) =>
                      `${slot.routeId} → ${migrationPreview.slots[i]?.routeId}；${t("aggregate.migrationLimits", { defaultValue: "上限" })} ${slot.maxEffort ?? t("aggregate.migrationUnlimited", { defaultValue: "不限制" })} → ${migrationPreview.slots[i]?.maxEffort ?? t("aggregate.migrationUnlimited", { defaultValue: "不限制" })}`,
                  )
                  .join("\n")}
              </pre>
              <p>
                {t("aggregate.defaultTarget", { defaultValue: "兜底目标" })}:{" "}
                {value.defaultTarget.value} →{" "}
                {migrationPreview.defaultTarget.value}
              </p>
              <p>
                {t("aggregate.defaultModel", { defaultValue: "默认模型" })}:{" "}
                {value.defaultModel ?? "—"} →{" "}
                {migrationPreview.defaultModel ?? "—"}
              </p>
              {(value.aliasRules ?? []).map((rule, i) => (
                <p key={i}>
                  {rule.prefix}: {rule.slotId || "—"} →{" "}
                  {migrationPreview.aliasRules?.[i]?.slotId || "—"}
                </p>
              ))}
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={downloadMigrationBackup}
              >
                {t("aggregate.migrationBackup", {
                  defaultValue: "下载路由备份",
                })}
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={!migrationBackup}
                onClick={() => {
                  onChange(migrationPreview);
                  setMigrationPreview(undefined);
                  setMigrationBackup(false);
                }}
              >
                {t("aggregate.migrationConfirm", {
                  defaultValue: "确认转换路由 ID",
                })}
              </Button>
            </>
          )}
        </div>
      )}

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

      {/* 别名路由规则：槽位未命中时按前缀转投。按序先赢，运行时小写化匹配。 */}
      <div className="space-y-2 rounded-md border border-border-default p-3">
        <p
          className="text-xs text-muted-foreground"
          title={t("aggregate.aliasTip", {
            defaultValue:
              "前缀过宽（如 claude-）会吞掉所有未命中槽位的请求名，注意范围",
          })}
        >
          {t("aggregate.aliasRouting", {
            defaultValue: "别名路由（槽位未命中时按前缀转投，先匹配先赢）",
          })}
        </p>
        {(value.aliasRules ?? []).map((rule, index) => (
          <div key={index} className="flex items-center gap-2">
            <Input
              className="h-8 w-56 shrink-0"
              value={rule.prefix}
              placeholder={t("aggregate.aliasPrefixPlaceholder", {
                defaultValue: "如 claude-sonnet",
              })}
              title={t("aggregate.aliasPrefix", { defaultValue: "请求名前缀" })}
              onChange={(e) =>
                patchAliasRule(index, { prefix: e.target.value })
              }
            />
            <span className="text-xs text-muted-foreground">→</span>
            <Select
              value={rule.slotId || UNSET}
              onValueChange={(v) =>
                patchAliasRule(index, { slotId: v === UNSET ? "" : v })
              }
            >
              <SelectTrigger
                className="h-8 flex-1"
                aria-label={t("aggregate.aliasTarget", {
                  defaultValue: "目标槽位",
                })}
              >
                <SelectValue
                  placeholder={t("aggregate.aliasTargetPlaceholder", {
                    defaultValue: "选择槽位",
                  })}
                />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={UNSET}>
                  {t("aggregate.aliasTargetPlaceholder", {
                    defaultValue: "选择槽位",
                  })}
                </SelectItem>
                {value.slots.map((slot) => (
                  <SelectItem key={slot.routeId} value={slot.routeId}>
                    {labelOf(slot)} ({slot.routeId})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-7 w-6 shrink-0 text-muted-foreground hover:text-destructive"
              title={t("aggregate.removeAliasRule", {
                defaultValue: "删除该规则",
              })}
              onClick={() => removeAliasRule(index)}
            >
              <X className="h-3.5 w-3.5" />
            </Button>
          </div>
        ))}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 gap-1 text-xs text-muted-foreground"
          onClick={addAliasRule}
        >
          <Plus className="h-3.5 w-3.5" />
          {t("aggregate.addAliasRule", { defaultValue: "添加规则" })}
        </Button>
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
              const mappedRows = TIER_ROW_ORDER.flatMap((tier) =>
                (card.rows[tier] ?? []).map((slot, rowIndex) => ({
                  tier,
                  slot,
                  rowIndex,
                })),
              );
              const availableTiers = TIER_ROW_ORDER;
              const expanded = expandedIds.has(card.providerId);
              // routeId 恒非空（每次 commit 都经 assignSlotIds 赋真 ID），无已映射
              // 槽位时 effectiveDefault 为 ""，恒不命中，无需再挡空值。
              const isDefaultCard = mappedRows.some(
                ({ slot }) => slot.routeId === effectiveDefault,
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
                            <span className="w-16 shrink-0">
                              {t("aggregate.effortBadgeHeader")}
                            </span>
                            <span className="min-w-0 flex-1">
                              {t("aggregate.upstreamModel")}
                            </span>
                            <span className="min-w-0 flex-1">
                              {t("aggregate.displayName")}
                            </span>
                            <span className="w-24 shrink-0 text-right">
                              {t("aggregate.maxEffort")}
                            </span>
                            <span className="w-20 shrink-0 text-right">
                              {t("aggregate.supports1m")}
                            </span>
                            <span className="w-6 shrink-0" />
                          </div>

                          {/* 已映射档位的行（v3：增量添加，不再固定四行）。行尾 ×
                              删除该槽；行内不支持改档位（删了重加）。 */}
                          {mappedRows.map(({ tier, slot, rowIndex }) => {
                            const isEffectiveDefault =
                              slot.routeId === effectiveDefault;
                            const switchId = `agg-1m-${slot.routeId || `${cardIndex}-${tier}-${rowIndex}`}`;
                            return (
                              <div
                                key={slot.routeId || `${tier}-${rowIndex}`}
                                className="flex items-center gap-2"
                                // 槽位 ID 不占版面，悬停可查——排查代理日志里的
                                // request_model 时用得上
                                title={slot.routeId || undefined}
                              >
                                <span className="w-14 shrink-0 text-xs text-muted-foreground">
                                  {tier}
                                </span>
                                <EffortBadge routeId={slot.routeId} />
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
                                      patchRow(cardIndex, tier, rowIndex, {
                                        upstreamModel: e.target.value,
                                      })
                                    }
                                  />
                                  {fetched.length > 0 && (
                                    <ModelDropdown
                                      models={fetched}
                                      onSelect={(id) =>
                                        patchRow(cardIndex, tier, rowIndex, {
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
                                      patchRow(cardIndex, tier, rowIndex, {
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
                                <MaxEffortSelect
                                  routeId={slot.routeId}
                                  value={slot.maxEffort}
                                  onChange={(v) =>
                                    patchRow(cardIndex, tier, rowIndex, {
                                      maxEffort: v,
                                    })
                                  }
                                />
                                <div className="flex w-20 shrink-0 items-center justify-end gap-1.5">
                                  <Switch
                                    id={switchId}
                                    checked={Boolean(slot.supports1m)}
                                    onCheckedChange={(v) =>
                                      patchRow(cardIndex, tier, rowIndex, {
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
                                  onClick={() =>
                                    removeRow(cardIndex, tier, rowIndex)
                                  }
                                >
                                  <X className="h-3.5 w-3.5" />
                                </Button>
                              </div>
                            );
                          })}

                          {/* 新增模型：各档位允许多行，不覆盖既有模型 */}
                          {availableTiers.length > 0 && (
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
                                {availableTiers.map((tier) => (
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
                        /* 折叠态摘要：N 个模型 · M 强度 */
                        <p className="text-xs text-muted-foreground">
                          {t("aggregate.modelsSummaryLadder", {
                            count: mappedRows.length,
                            ladder: mappedRows.filter(
                              ({ slot }) =>
                                effortCapability(slot.routeId) === "ladder",
                            ).length,
                            defaultValue: "{{count}} 个模型 · {{ladder}} 强度",
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

function EffortBadge({ routeId }: { routeId: string }) {
  const { t } = useTranslation();
  // 测试环境 i18n 是空资源：t() 必须带 defaultValue，否则返回键名本身、组件测试必挂
  const conf = {
    ladder: {
      key: "effortLadder",
      tipKey: "effortLadderTip",
      cls: "text-emerald-600 dark:text-emerald-400",
      text: "强度✓",
      tip: "完整思考强度阶梯（low…max），来自真模型 ID",
    },
    extended: {
      key: "effortToggle",
      tipKey: "effortToggleTip",
      cls: "text-muted-foreground",
      text: "思考开关",
      tip: "仅扩展思考开/关，无强度档位",
    },
    none: {
      key: "effortNone",
      tipKey: "effortNoneTip",
      cls: "text-muted-foreground/50",
      text: "✗",
      tip: "溢出 ID：Claude Desktop 不认识，无思考控件",
    },
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

/** 槽位行的思考强度上限下拉：只有完整阶梯 ID 有档位可选，其余禁用并给出原因。 */
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
      onValueChange={(v) =>
        onChange(v === UNSET ? undefined : (v as AggregateMaxEffort))
      }
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
