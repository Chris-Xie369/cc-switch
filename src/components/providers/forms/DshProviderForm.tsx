import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
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
import type { ProviderFormProps, ProviderFormValues } from "./ProviderForm";
import { ProviderPresetSelector } from "./ProviderPresetSelector";
import { dshProviderPresets } from "@/config/dshProviderPresets";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useProvidersQuery } from "@/lib/query/queries";
import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";

/** DeepSeek Harness 供应商卡：settings_config 即 pi-ai provider 形状
 *  {displayName, api, baseUrl, apiKey, models:[{id,name}]}，
 *  保存时由后端 write_provider_card 落进 ~/.dsh/settings.yaml + 凭据 refs。 */

interface ModelRow {
  id: string;
  name: string;
  reasoning?: boolean;
  input?: string[];
  contextWindow?: number;
  maxTokens?: number;
}

const DSH_KEY_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/;

const API_OPTIONS = [
  { value: "anthropic-messages", label: "Anthropic Messages" },
  { value: "openai-completions", label: "OpenAI Chat Completions" },
  { value: "openai-responses", label: "OpenAI Responses" },
] as const;

export function DshProviderForm({
  providerId,
  submitLabel,
  onSubmit,
  onCancel,
  initialData,
  onSubmittingChange,
  onSubmitReadyChange,
}: ProviderFormProps) {
  const { t } = useTranslation();
  const isEdit = Boolean(initialData);
  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(
    isEdit ? null : "custom",
  );
  const presetEntries = useMemo(
    () =>
      dshProviderPresets.map((preset, index) => ({
        id: `dsh-${index}`,
        preset,
      })),
    [],
  );
  const presetCategoryLabels = useMemo(
    () => ({
      aggregator: t("providerForm.categoryAggregation", {
        defaultValue: "聚合服务",
      }),
      third_party: t("providerForm.categoryThirdParty", {
        defaultValue: "第三方",
      }),
    }),
    [t],
  );
  const selectPreset = useCallback((id: string) => {
    setSelectedPresetId(id);
    setFetchedModels(null);
    if (id === "custom") {
      setName("");
      setProviderKey("");
      setDisplayName("");
      setApi("anthropic-messages");
      setBaseUrl("");
      setModels([{ id: "", name: "" }]);
      return;
    }
    const entry = dshProviderPresets[Number(id.replace("dsh-", ""))];
    if (!entry) return;
    const cfg = entry.settingsConfig;
    setName(entry.name);
    setProviderKey(entry.providerKey);
    setDisplayName(cfg.displayName ?? entry.name);
    setApi(cfg.api);
    setBaseUrl(cfg.baseUrl);
    setModels(cfg.models.map((m) => ({ ...m })));
  }, []);
  const [name, setName] = useState(initialData?.name ?? "");
  const [displayName, setDisplayName] = useState(
    (initialData?.settingsConfig?.displayName as string | undefined) ??
      initialData?.name ??
      "",
  );
  const [api, setApi] = useState<string>(
    (initialData?.settingsConfig?.api as string | undefined) ??
      "anthropic-messages",
  );
  const [baseUrl, setBaseUrl] = useState(
    (initialData?.settingsConfig?.baseUrl as string | undefined) ?? "",
  );
  const [apiKey, setApiKey] = useState(
    (initialData?.settingsConfig?.apiKey as string | undefined) ?? "",
  );
  const [models, setModels] = useState<ModelRow[]>(() => {
    const rows = initialData?.settingsConfig?.models;
    if (Array.isArray(rows)) {
      return rows.map((row) => {
        const r = row as Record<string, unknown>;
        return {
          id: String(r.id ?? ""),
          name: String(r.name ?? ""),
          reasoning: typeof r.reasoning === "boolean" ? r.reasoning : undefined,
          input: Array.isArray(r.input) ? (r.input as string[]) : undefined,
          contextWindow:
            typeof r.contextWindow === "number" ? r.contextWindow : undefined,
          maxTokens: typeof r.maxTokens === "number" ? r.maxTokens : undefined,
        };
      });
    }
    return [{ id: "", name: "" }];
  });
  const [providerKey, setProviderKey] = useState(providerId ?? "");
  const { data: existing } = useProvidersQuery("deepseek-harness");
  const keyTaken =
    !isEdit &&
    Object.prototype.hasOwnProperty.call(
      existing?.providers ?? {},
      providerKey,
    );
  const keyInvalid =
    !isEdit && providerKey !== "" && !DSH_KEY_PATTERN.test(providerKey);
  const [expandedRows, setExpandedRows] = useState<Set<number>>(new Set());
  const toggleRow = (index: number) =>
    setExpandedRows((current) => {
      const next = new Set(current);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const [fetchedModels, setFetchedModels] = useState<FetchedModel[] | null>(
    null,
  );
  const [fetchingModels, setFetchingModels] = useState(false);
  const handleFetchModels = async () => {
    if (!baseUrl.trim() || !apiKey.trim()) {
      showFetchModelsError(null, t, {
        hasBaseUrl: Boolean(baseUrl.trim()),
        hasApiKey: Boolean(apiKey.trim()),
      });
      return;
    }
    setFetchingModels(true);
    try {
      const models = await fetchModelsForConfig(baseUrl.trim(), apiKey.trim());
      setFetchedModels(models);
      toast.success(
        t("providerForm.fetchModelsSuccess", {
          count: models.length,
          defaultValue: `已获取 ${models.length} 个模型`,
        }),
      );
    } catch (error) {
      showFetchModelsError(error, t, {
        hasBaseUrl: Boolean(baseUrl.trim()),
        hasApiKey: Boolean(apiKey.trim()),
      });
    } finally {
      setFetchingModels(false);
    }
  };

  const ready = useMemo(
    () =>
      name.trim().length > 0 &&
      (isEdit || (providerKey.trim().length > 0 && !keyInvalid && !keyTaken)) &&
      baseUrl.trim().length > 0 &&
      models.some((m) => m.id.trim().length > 0),
    [name, baseUrl, models],
  );

  useEffect(() => {
    onSubmitReadyChange?.(ready);
  }, [ready, onSubmitReadyChange]);

  const setRow = useCallback((index: number, patch: Partial<ModelRow>) => {
    setModels((rows) =>
      rows.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    );
  }, []);

  const handleSubmit = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      if (!ready || submitting) return;
      setSubmitting(true);
      onSubmittingChange?.(true);
      setSubmitError(null);
      try {
        const cleaned = models
          .filter((m) => m.id.trim().length > 0)
          .map((m) => {
            const base = {
              id: m.id.trim(),
              name: (m.name || m.id).trim(),
            };
            return {
              ...base,
              ...(m.reasoning !== undefined ? { reasoning: m.reasoning } : {}),
              ...(m.input ? { input: m.input } : {}),
              ...(m.contextWindow !== undefined
                ? { contextWindow: m.contextWindow }
                : {}),
              ...(m.maxTokens !== undefined ? { maxTokens: m.maxTokens } : {}),
            };
          });
        const values: ProviderFormValues = {
          name: name.trim(),
          websiteUrl: "",
          notes: initialData?.notes ?? "",
          settingsConfig: JSON.stringify({
            displayName: displayName.trim() || name.trim(),
            api,
            baseUrl: baseUrl.trim(),
            // 编辑时留空 = 不改凭据（后端跳过空 key 的 ref 写入）
            apiKey: apiKey.trim(),
            models: cleaned,
          }),
        };
        await onSubmit(values);
      } catch (error) {
        setSubmitError(extractErrorMessage(error) || t("common.error"));
      } finally {
        setSubmitting(false);
        onSubmittingChange?.(false);
      }
    },
    [
      ready,
      submitting,
      name,
      displayName,
      api,
      baseUrl,
      apiKey,
      models,
      initialData?.notes,
      onSubmit,
      onSubmittingChange,
      t,
    ],
  );

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4">
      {!isEdit && (
        <ProviderPresetSelector
          selectedPresetId={selectedPresetId}
          presetEntries={presetEntries}
          presetCategoryLabels={presetCategoryLabels}
          onPresetChange={selectPreset}
          category="custom"
        />
      )}
      {!isEdit && (
        <div className="grid gap-2">
          <Label htmlFor="dsh-provider-key">{t("opencode.providerKey")}</Label>
          <Input
            id="dsh-provider-key"
            value={providerKey}
            onChange={(e) => setProviderKey(e.target.value)}
            placeholder={t("opencode.providerKeyPlaceholder")}
            className={
              keyTaken || keyInvalid ? "border-destructive" : undefined
            }
          />
          {keyTaken ? (
            <p className="text-sm text-destructive">
              {t("opencode.providerKeyDuplicate")}
            </p>
          ) : keyInvalid ? (
            <p className="text-sm text-destructive">
              {t("opencode.providerKeyInvalid")}
            </p>
          ) : (
            <p className="text-xs text-fg-3">{t("opencode.providerKeyHint")}</p>
          )}
        </div>
      )}
      <div className="grid gap-2">
        <Label htmlFor="dsh-name">{t("providerForm.supplierName")}</Label>
        <Input
          id="dsh-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="My Gateway"
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="dsh-display-name">
          {t("deepseekHarness.form.displayName")}
        </Label>
        <Input
          id="dsh-display-name"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          placeholder={t("deepseekHarness.form.displayNamePlaceholder")}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="dsh-api">{t("deepseekHarness.form.api")}</Label>
        <Select value={api} onValueChange={setApi}>
          <SelectTrigger id="dsh-api" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {API_OPTIONS.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="dsh-base-url">Base URL</Label>
        <Input
          id="dsh-base-url"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://gw.example/v1"
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="dsh-api-key">API Key</Label>
        <Input
          id="dsh-api-key"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder={
            providerId
              ? t("deepseekHarness.form.apiKeyKeepPlaceholder")
              : "sk-..."
          }
        />
      </div>
      <div className="grid gap-2">
        <div className="flex items-center justify-between">
          <Label>{t("deepseekHarness.form.models")}</Label>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleFetchModels}
            disabled={fetchingModels}
          >
            {fetchingModels
              ? t("providerForm.fetchingModels")
              : t("deepseekHarness.form.fetchModels")}
          </Button>
        </div>
        {fetchedModels && fetchedModels.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {fetchedModels.map((model) => (
              <Button
                key={model.id}
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                  setModels((rows) =>
                    rows.some((r) => r.id === model.id)
                      ? rows
                      : [
                          ...rows.filter((r) => r.id.trim().length > 0),
                          { id: model.id, name: model.id },
                        ],
                  )
                }
              >
                + {model.id}
              </Button>
            ))}
          </div>
        ) : null}
        {models.map((row, index) => (
          <div key={index} className="space-y-2">
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={t("deepseekHarness.form.modelMeta")}
                onClick={() => toggleRow(index)}
                className="shrink-0"
              >
                {expandedRows.has(index) ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
              </Button>
              <Input
                value={row.id}
                onChange={(e) => setRow(index, { id: e.target.value })}
                placeholder="model-id"
                className="flex-1"
              />
              <Input
                value={row.name}
                onChange={(e) => setRow(index, { name: e.target.value })}
                placeholder={t("deepseekHarness.form.modelNamePlaceholder")}
                className="flex-1"
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                onClick={() => {
                  setModels((rows) =>
                    rows.length > 1
                      ? rows.filter((_, i) => i !== index)
                      : [{ id: "", name: "" }],
                  );
                  setExpandedRows((current) => {
                    const next = new Set(current);
                    next.delete(index);
                    return next;
                  });
                }}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
            {expandedRows.has(index) && (
              <div className="grid grid-cols-2 gap-2 rounded-lg border border-border p-3">
                <div className="grid gap-1.5">
                  <Label className="text-xs">
                    {t("deepseekHarness.form.contextWindow")}
                  </Label>
                  <Input
                    type="number"
                    value={row.contextWindow ?? ""}
                    onChange={(e) =>
                      setRow(index, {
                        contextWindow: e.target.value
                          ? Number(e.target.value)
                          : undefined,
                      })
                    }
                    placeholder="1000000"
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label className="text-xs">
                    {t("deepseekHarness.form.maxTokens")}
                  </Label>
                  <Input
                    type="number"
                    value={row.maxTokens ?? ""}
                    onChange={(e) =>
                      setRow(index, {
                        maxTokens: e.target.value
                          ? Number(e.target.value)
                          : undefined,
                      })
                    }
                    placeholder="384000"
                  />
                </div>
                <div className="flex items-center gap-2">
                  <Switch
                    id={`dsh-reasoning-${index}`}
                    checked={row.reasoning ?? false}
                    onCheckedChange={(checked) =>
                      setRow(index, { reasoning: checked })
                    }
                  />
                  <Label htmlFor={`dsh-reasoning-${index}`} className="text-xs">
                    {t("deepseekHarness.form.reasoning")}
                  </Label>
                </div>
                <div className="grid gap-1.5">
                  <Label className="text-xs">
                    {t("deepseekHarness.form.inputModalities")}
                  </Label>
                  <Input
                    value={(row.input ?? []).join(",")}
                    onChange={(e) =>
                      setRow(index, {
                        input: e.target.value
                          .split(",")
                          .map((s) => s.trim())
                          .filter(Boolean),
                      })
                    }
                    placeholder="text, image"
                  />
                </div>
              </div>
            )}
          </div>
        ))}
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="w-fit"
          onClick={() => setModels((rows) => [...rows, { id: "", name: "" }])}
        >
          <Plus className="h-4 w-4" />
          {t("deepseekHarness.form.addModel")}
        </Button>
      </div>
      <div className="grid gap-2">
        <Label>{t("deepseekHarness.form.configPreview")}</Label>
        <pre className="max-h-64 overflow-auto rounded-lg border border-border bg-muted/30 p-3 text-xs leading-relaxed">
          {JSON.stringify(
            {
              displayName: displayName.trim() || name.trim(),
              api,
              baseUrl: baseUrl.trim(),
              apiKey: apiKey.trim() ? "***" : "",
              models: models
                .filter((m) => m.id.trim().length > 0)
                .map((m) => ({
                  id: m.id.trim(),
                  name: (m.name || m.id).trim(),
                  ...(m.reasoning !== undefined ? { reasoning: m.reasoning } : {}),
                  ...(m.input?.length ? { input: m.input } : {}),
                  ...(m.contextWindow !== undefined ? { contextWindow: m.contextWindow } : {}),
                  ...(m.maxTokens !== undefined ? { maxTokens: m.maxTokens } : {}),
                })),
            },
            null,
            2,
          )}
        </pre>
      </div>
      {submitError ? (
        <p className="text-sm text-red-500">{submitError}</p>
      ) : null}
      <div className="flex justify-end gap-2 pt-2">
        <Button type="button" variant="outline" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
        <Button type="submit" disabled={!ready || submitting}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
