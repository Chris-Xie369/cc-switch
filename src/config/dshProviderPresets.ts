//! DeepSeek Harness 供应商预设——与 mcodeProviderPresets 完全同构的派生：
//! 从 piProviderPresets 复用端点/模型元数据，产出 OpenCode 格式的
//! settingsConfig（嵌套 models map + options），OpenCodeFormFields 直接消费。

import {
  piProviderPresets,
  type PiApiFormat,
  type PiPresetModel,
} from "./piProviderPresets";
import type { PresetFamilyFields } from "./presetFamilies";
import type { ProviderCategory, OpenCodeModel } from "@/types";

export type DshApiFormat = Exclude<
  PiApiFormat,
  "google-generative-ai" | "bedrock-converse-stream"
>;

export interface DshProviderPreset extends PresetFamilyFields {
  providerKey: string;
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: {
    name: string;
    kind: string;
    enabled: boolean;
    api: DshApiFormat;
    options: {
      baseURL: string;
      apiKey: string;
      headers?: Record<string, string>;
    };
    models: Record<string, OpenCodeModel>;
  };
  category?: ProviderCategory;
  icon?: string;
  iconColor?: string;
  hidden?: boolean;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  theme?: { icon?: string };
}

const DSH_API_FORMATS: readonly DshApiFormat[] = [
  "anthropic-messages",
  "openai-completions",
  "openai-responses",
];

function presetModel(model: PiPresetModel): OpenCodeModel {
  return {
    name: model.name,
    reasoning: model.reasoning,
    modalities: { input: model.input, output: ["text"] },
    limit: {
      context: model.contextWindow,
      output: model.maxTokens,
    },
  };
}

export const dshProviderPresets: DshProviderPreset[] =
  piProviderPresets.flatMap((preset) => {
    const config = preset.settingsConfig;
    const api = DSH_API_FORMATS.find((format) => format === config.api);
    if (!api) return [];
    const models = config.models;
    if (models.length === 0) return [];
    return [
      {
        ...preset,
        settingsConfig: {
          name: preset.name,
          kind: "custom",
          enabled: true,
          api,
          options: {
            baseURL: config.baseUrl,
            apiKey: "",
            ...(config.headers ? { headers: config.headers } : {}),
          },
          models: Object.fromEntries(
            models.map((model) => [model.id, presetModel(model)]),
          ),
        },
      },
    ];
  });
