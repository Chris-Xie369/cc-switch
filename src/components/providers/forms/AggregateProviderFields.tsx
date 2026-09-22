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
  slotLabel,
} from "@/utils/aggregateRoutes";

const TIERS: AggregateTier[] = ["sonnet", "opus", "haiku", "fable"];

interface Props {
  value: AggregateRoutes;
  onChange: (next: AggregateRoutes) => void;
  /** 可作目标的常规供应商（不含聚合供应商自身与官方供应商，防止嵌套/无凭据目标） */
  candidates: Provider[];
  /** 该目标供应商已拉取到的模型列表；按供应商 id 缓存，同家的多个槽位共用 */
  modelsForProvider: (providerId: string) => FetchedModel[];
  /** 正在拉取的供应商 id；非 null 时所有拉取按钮进入 loading */
  fetchingProviderId: string | null;
  onFetchModels: (provider: Provider) => void;
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

  // 每次变更都重算所有槽位 ID：保证「预览 = 实际提交值」。
  // assignSlotIds 会按档位从零重编号，并让引用槽位 ID 的默认目标按位置跟随，
  // 故改档位/增删槽位后引用不会悬空（目标槽位被删则保持原值，交由保存校验拦截）。
  const commit = (next: AggregateRoutes) => onChange(assignSlotIds(next));

  const setSlot = (index: number, patch: Partial<AggregateRouteSlot>) => {
    const slots = value.slots.map((s, i) =>
      i === index ? { ...s, ...patch } : s,
    );
    commit({ ...value, slots });
  };
  const addSlot = () =>
    commit({
      ...value,
      slots: [
        ...value.slots,
        {
          routeId: "",
          tier: "sonnet",
          providerId: candidates[0]?.id ?? "",
          upstreamModel: "",
          supports1m: false,
        },
      ],
    });
  const removeSlot = (index: number) =>
    commit({ ...value, slots: value.slots.filter((_, i) => i !== index) });

  return (
    <section className="space-y-4">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">{t("aggregate.title")}</h3>
        <p className="text-xs text-muted-foreground">{t("aggregate.hint")}</p>
      </header>

      {!canSaveAggregateRoutes(value) && (
        <p className="text-xs text-destructive">{t("aggregate.notSaveable")}</p>
      )}

      {/* 默认目标是必填项，放在槽位**之前**：原先它在槽位列表下方，槽位一多就被顶出
          视野、又渲染成一个没有占位文案的空白下拉框，用户会卡在这里（2026-09-22 实测）。
          它的选项来自供应商与槽位，槽位为空时先选一家供应商也成立。 */}
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
                {slotLabel(s)} ({s.routeId})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">
          {t("aggregate.defaultTargetHint")}
        </p>
      </div>

      <div className="space-y-3">
        {value.slots.map((slot, index) => (
          <div
            key={index}
            className="space-y-2 rounded-md border border-border-default p-3"
          >
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <Label className="text-xs">{t("aggregate.tier")}</Label>
                <Select
                  value={slot.tier}
                  onValueChange={(v) =>
                    setSlot(index, { tier: v as AggregateTier })
                  }
                >
                  <SelectTrigger className="h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {TIERS.map((tier) => (
                      <SelectItem key={tier} value={tier}>
                        {tier}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1">
                <Label className="text-xs">
                  {t("aggregate.targetProvider")}
                </Label>
                <Select
                  value={slot.providerId}
                  onValueChange={(v) => setSlot(index, { providerId: v })}
                >
                  <SelectTrigger className="h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {candidates.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <div className="flex items-center justify-between gap-2">
                  <Label className="text-xs">
                    {t("aggregate.upstreamModel")}
                  </Label>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-7 gap-1"
                    disabled={!slot.providerId || fetchingProviderId !== null}
                    onClick={() => {
                      const target = candidates.find(
                        (p) => p.id === slot.providerId,
                      );
                      if (target) onFetchModels(target);
                    }}
                  >
                    {fetchingProviderId === slot.providerId ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Download className="h-3.5 w-3.5" />
                    )}
                    {t("providerForm.fetchModels", {
                      defaultValue: "获取模型列表",
                    })}
                  </Button>
                </div>
                <div className="flex gap-1">
                  <Input
                    className="h-8 flex-1"
                    value={slot.upstreamModel}
                    onChange={(e) =>
                      setSlot(index, { upstreamModel: e.target.value })
                    }
                  />
                  {modelsForProvider(slot.providerId).length > 0 && (
                    <ModelDropdown
                      models={modelsForProvider(slot.providerId)}
                      onSelect={(id) => setSlot(index, { upstreamModel: id })}
                    />
                  )}
                </div>
              </div>
              <div className="space-y-1">
                <Label className="text-xs">{t("aggregate.displayName")}</Label>
                <ImeSafeInput
                  className="h-8"
                  placeholder={slotLabel(slot)}
                  value={slot.label ?? ""}
                  // 空显示名回落为 undefined（省略该键），后端视同未填写。
                  onValueChange={(v) =>
                    setSlot(index, { label: v.trim() ? v : undefined })
                  }
                />
              </div>
            </div>

            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Switch
                  id={`agg-1m-${index}`}
                  checked={Boolean(slot.supports1m)}
                  onCheckedChange={(v) => setSlot(index, { supports1m: v })}
                />
                <Label htmlFor={`agg-1m-${index}`} className="text-xs">
                  {t("aggregate.supports1m")}
                </Label>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => removeSlot(index)}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>

            <p className="text-xs text-muted-foreground">
              {t("aggregate.slotIdPreview")}: <code>{slot.routeId}</code>
            </p>
          </div>
        ))}
      </div>

      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 gap-1.5"
        onClick={addSlot}
      >
        <Plus className="h-3.5 w-3.5" />
        {t("aggregate.addSlot")}
      </Button>
    </section>
  );
}
