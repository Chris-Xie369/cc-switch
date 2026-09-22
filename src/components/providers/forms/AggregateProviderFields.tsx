import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ImeSafeInput } from "@/components/ui/ime-safe-input";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
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
import { assignSlotIds, slotLabel } from "@/utils/aggregateRoutes";

const TIERS: AggregateTier[] = ["sonnet", "opus", "haiku", "fable"];

interface Props {
  value: AggregateRoutes;
  onChange: (next: AggregateRoutes) => void;
  /** 可作目标的常规供应商（不含聚合供应商自身，防止嵌套） */
  candidates: Provider[];
}

export function AggregateProviderFields({
  value,
  onChange,
  candidates,
}: Props) {
  const { t } = useTranslation();

  // 每次变更都重算所有槽位 ID：保证「预览 = 实际提交值」
  const commit = (next: AggregateRoutes) =>
    onChange(assignSlotIds(next, candidates));

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
                <Label className="text-xs">
                  {t("aggregate.upstreamModel")}
                </Label>
                <Input
                  className="h-8"
                  value={slot.upstreamModel}
                  onChange={(e) =>
                    setSlot(index, { upstreamModel: e.target.value })
                  }
                />
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
            <SelectValue />
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
    </section>
  );
}
