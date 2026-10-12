//! DeepSeek Harness 供应商预设——直接从 piProviderPresets 派生（DSH 与 Pi 同为
//! pi-ai 下游，baseUrl/api/models 元数据一一对应），与 mcodeProviderPresets 的
//! 派生模式相同：复用 Pi 的端点/模型元数据，仅按 DSH 支持的协议过滤。

import { piProviderPresets, type PiPresetModel } from "./piProviderPresets";
import type { PresetFamilyFields } from "./presetFamilies";
import type { ProviderCategory } from "@/types";

export type DshApiFormat =
  | "anthropic-messages"
  | "openai-completions"
  | "openai-responses";

export interface DshProviderPreset extends PresetFamilyFields {
  /** patch 条目里的 route 名（providers.<route>），小写连字符。 */
  providerKey: string;
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: {
    displayName?: string;
    api: DshApiFormat;
    baseUrl: string;
    models: DshPresetModel[];
  };
  category?: ProviderCategory;
  icon?: string;
  iconColor?: string;
  hidden?: boolean;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  theme?: { icon?: string };
}

/** pi-ai 的 models 条目原生支持这些容量字段（cordis.patch.yml 直读），全量保留。 */
export interface DshPresetModel {
  id: string;
  name: string;
  reasoning?: boolean;
  input?: string[];
  contextWindow?: number;
  maxTokens?: number;
}

function presetModel(model: PiPresetModel): DshPresetModel {
  return {
    id: model.id,
    name: model.name,
    reasoning: model.reasoning,
    input: model.input,
    contextWindow: model.contextWindow,
    maxTokens: model.maxTokens,
  };
}

const DSH_API_FORMATS: readonly DshApiFormat[] = [
  "anthropic-messages",
  "openai-completions",
  "openai-responses",
];

export const dshProviderPresets: DshProviderPreset[] =
  piProviderPresets.flatMap((preset) => {
    const config = preset.settingsConfig;
    const api = DSH_API_FORMATS.find((format) => format === config.api);
    if (!api) return [];
    return [
      {
        ...preset,
        settingsConfig: {
          displayName: config.name,
          api,
          baseUrl: config.baseUrl,
          models: config.models.map(presetModel),
        },
      },
    ];
  });
