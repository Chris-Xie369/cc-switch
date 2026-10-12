import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { ProviderFormProps, ProviderFormValues } from "./ProviderForm";
import { extractErrorMessage } from "@/utils/errorUtils";

/** DeepSeek Harness 供应商卡：settings_config 即 pi-ai provider 形状
 *  {displayName, api, baseUrl, apiKey, models:[{id,name}]}，
 *  保存时由后端 write_provider_card 落进 ~/.dsh/settings.yaml + 凭据 refs。 */

interface ModelRow {
  id: string;
  name: string;
}

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
      return rows.map((row) => ({
        id: String((row as Record<string, unknown>).id ?? ""),
        name: String((row as Record<string, unknown>).name ?? ""),
      }));
    }
    return [{ id: "", name: "" }];
  });
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const ready = useMemo(
    () =>
      name.trim().length > 0 &&
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
          .map((m) => ({ id: m.id.trim(), name: (m.name || m.id).trim() }));
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
      <div className="grid gap-2">
        <Label htmlFor="dsh-name">{t("providerForm.name")}</Label>
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
        <Label>{t("deepseekHarness.form.models")}</Label>
        {models.map((row, index) => (
          <div key={index} className="flex items-center gap-2">
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
              onClick={() =>
                setModels((rows) =>
                  rows.length > 1
                    ? rows.filter((_, i) => i !== index)
                    : [{ id: "", name: "" }],
                )
              }
            >
              <Trash2 className="h-4 w-4" />
            </Button>
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
