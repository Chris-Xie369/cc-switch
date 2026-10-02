import { useMemo } from "react";
import { Download, Loader2, Plus, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ImeSafeInput } from "@/components/ui/ime-safe-input";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ModelDropdown } from "./shared/ModelDropdown";
import type { FetchedModel } from "@/lib/api/model-fetch";
import type {
  CodexAggregateRoutes,
  CodexAggregateSlot,
  DefaultTarget,
  Provider,
} from "@/types";

interface Props {
  value: CodexAggregateRoutes;
  onChange: (next: CodexAggregateRoutes) => void;
  /** 可作目标的常规 Codex 供应商（不含聚合供应商自身与官方供应商） */
  candidates: Provider[];
  /** 该目标供应商已拉取到的模型列表；按供应商 id 缓存，同一供应商的多行共用 */
  modelsForProvider: (providerId: string) => FetchedModel[];
  /** 正在拉取的供应商 id；非 null 时所有拉取按钮进入 loading */
  fetchingProviderId: string | null;
  onFetchModels: (provider: Provider) => void;
}

/** Radix Select 不接受空选项值：「未设置」用哨兵值占位。 */
const UNSET = "__unset__";

export function CodexAggregateFields({
  value,
  onChange,
  candidates,
  modelsForProvider,
  fetchingProviderId,
  onFetchModels,
}: Props) {
  const { t } = useTranslation();

  // 显示名留空时的占位：同一个上游模型可能挂在两家供应商上，只写模型名
  // 仍分不清实际调用的是哪一家（与 Claude 侧聚合同一理由）。
  // 纯空白 label 按未设置处理——后端写模型目录时同样 trim 后丢弃它，
  // 若这里照原样占位会显示成一片空白。
  const providerNameById = new Map(candidates.map((p) => [p.id, p.name]));
  const labelOf = (slot: CodexAggregateSlot) =>
    slot.label?.trim() ||
    `${providerNameById.get(slot.providerId) ?? slot.providerId} · ${slot.upstreamModel}`;

  const patchSlot = (index: number, patch: Partial<CodexAggregateSlot>) => {
    const next = value.slots.map((slot, i) =>
      i === index ? { ...slot, ...patch } : slot,
    );
    onChange({ ...value, slots: next });
  };

  const removeSlot = (index: number) =>
    onChange({ ...value, slots: value.slots.filter((_, i) => i !== index) });

  const addSlot = () =>
    onChange({
      ...value,
      slots: [...value.slots, { model: "", providerId: "", upstreamModel: "" }],
    });

  // 槽位 model 可能有重复（用户正在输入）或空串（半成品行）。后端按 trim 后
  // 判重与判空，UI 不做硬门禁（与 Claude 侧一致：保存时 toast 拦截），
  // 但下拉项按 model 值去重，避免 Radix 同值项互相吞掉选中态。
  const slotModels = useMemo(() => {
    const seen = new Set<string>();
    const models: string[] = [];
    for (const slot of value.slots) {
      const model = slot.model.trim();
      if (!model || seen.has(model)) continue;
      seen.add(model);
      models.push(model);
    }
    return models;
  }, [value.slots]);

  return (
    <section className="space-y-4">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">
          {t("codexAggregate.title", { defaultValue: "聚合供应商" })}
        </h3>
        <p className="text-xs text-muted-foreground">
          {t("codexAggregate.hint", {
            defaultValue: "自身不存端点与密钥，按模型把请求分流到其他供应商。",
          })}
        </p>
      </header>

      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <Label className="text-xs">
            {t("codexAggregate.defaultTarget", { defaultValue: "兜底目标" })}
          </Label>
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
                placeholder={t("codexAggregate.defaultTargetPlaceholder", {
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
              {value.slots.map((slot, index) => (
                <SelectItem
                  key={`slot-${index}`}
                  value={`slot:${slot.model}`}
                  disabled={!slot.model.trim()}
                >
                  {labelOf(slot)} ({slot.model})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {t("codexAggregate.defaultTargetHint", {
              defaultValue: "未命中任何槽位的请求发往这里",
            })}
          </p>
        </div>

        <div className="space-y-1">
          <Label className="text-xs">
            {t("codexAggregate.defaultModel", { defaultValue: "默认模型" })}
          </Label>
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
                placeholder={t("codexAggregate.defaultModelPlaceholder", {
                  defaultValue: "未设置：跟随槽位首位",
                })}
              />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={UNSET}>
                {t("codexAggregate.defaultModelPlaceholder", {
                  defaultValue: "未设置：跟随槽位首位",
                })}
              </SelectItem>
              {slotModels.map((model) => (
                <SelectItem key={model} value={model}>
                  {model}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {t("codexAggregate.defaultModelHint", {
              defaultValue: "Codex CLI 默认选中的模型；未设置时取槽位第一条",
            })}
          </p>
        </div>
      </div>

      {value.slots.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("codexAggregate.emptyState", {
            defaultValue: "还没有槽位，点击下方「添加模型」开始配置。",
          })}
        </p>
      ) : (
        <div className="space-y-2">
          {/* 列头：模型名 | 目标供应商 | 上游模型 | 显示名 | 行尾删除 */}
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="min-w-0 flex-1">
              {t("codexAggregate.model", { defaultValue: "模型名" })}
            </span>
            <span className="w-40 shrink-0">
              {t("codexAggregate.targetProvider", {
                defaultValue: "目标供应商",
              })}
            </span>
            <span className="min-w-0 flex-1">
              {t("codexAggregate.upstreamModel", { defaultValue: "上游模型" })}
            </span>
            <span className="min-w-0 flex-1">
              {t("codexAggregate.displayName", { defaultValue: "显示名" })}
            </span>
            <span className="w-6 shrink-0" />
          </div>

          {value.slots.map((slot, index) => {
            const fetched = slot.providerId
              ? modelsForProvider(slot.providerId)
              : [];
            const target = candidates.find((p) => p.id === slot.providerId);
            return (
              <div key={index} className="flex items-center gap-2">
                <Input
                  className="h-8 min-w-0 flex-1"
                  value={slot.model}
                  placeholder={t("codexAggregate.modelPlaceholder", {
                    defaultValue: "客户端模型名",
                  })}
                  onChange={(e) => patchSlot(index, { model: e.target.value })}
                />
                <Select
                  value={slot.providerId || UNSET}
                  onValueChange={(v) =>
                    patchSlot(index, { providerId: v === UNSET ? "" : v })
                  }
                >
                  <SelectTrigger className="h-8 w-40 shrink-0">
                    <SelectValue
                      placeholder={t(
                        "codexAggregate.targetProviderPlaceholder",
                        { defaultValue: "选择供应商" },
                      )}
                    />
                  </SelectTrigger>
                  <SelectContent>
                    {candidates.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                    {/* 存量数据可能引用已被删除 / 已被改造成聚合的供应商：保底
                        显示，避免选择器空白让用户误以为没配置 */}
                    {slot.providerId &&
                      !candidates.some((p) => p.id === slot.providerId) && (
                        <SelectItem value={slot.providerId}>
                          {providerNameById.get(slot.providerId) ??
                            slot.providerId}
                        </SelectItem>
                      )}
                  </SelectContent>
                </Select>
                <div className="flex min-w-0 flex-1 gap-1">
                  <Input
                    className="h-8 min-w-0 flex-1"
                    value={slot.upstreamModel}
                    placeholder={t("codexAggregate.upstreamModelPlaceholder", {
                      defaultValue: "上游模型名",
                    })}
                    onChange={(e) =>
                      patchSlot(index, { upstreamModel: e.target.value })
                    }
                  />
                  {fetched.length > 0 && (
                    <ModelDropdown
                      models={fetched}
                      onSelect={(id) => patchSlot(index, { upstreamModel: id })}
                    />
                  )}
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    className="h-8 w-8 shrink-0"
                    title={t("codexAggregate.fetchModels", {
                      defaultValue: "获取模型列表",
                    })}
                    disabled={!target || fetchingProviderId !== null}
                    onClick={() => target && onFetchModels(target)}
                  >
                    {fetchingProviderId === slot.providerId ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Download className="h-3.5 w-3.5" />
                    )}
                  </Button>
                </div>
                {/* 显示名走 ImeSafeInput：中文输入法的组合态期间父组件重渲染
                    会冲掉浏览器管理的组合区间（同 Claude 侧聚合） */}
                <ImeSafeInput
                  className="h-8 min-w-0 flex-1"
                  placeholder={labelOf(slot)}
                  value={slot.label ?? ""}
                  // 留空则不落库：提交形状要求 label 为 undefined 而非空串
                  onValueChange={(v) =>
                    patchSlot(index, { label: v.trim() ? v : undefined })
                  }
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-6 shrink-0 text-muted-foreground hover:text-destructive"
                  title={t("codexAggregate.removeSlot", {
                    defaultValue: "删除该模型",
                  })}
                  onClick={() => removeSlot(index)}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </div>
            );
          })}
        </div>
      )}

      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 gap-1.5"
        onClick={addSlot}
      >
        <Plus className="h-3.5 w-3.5" />
        {t("codexAggregate.addSlot", { defaultValue: "添加模型" })}
      </Button>
    </section>
  );
}
