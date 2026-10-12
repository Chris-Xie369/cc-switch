//! DeepSeek Harness 供应商预设。形状与 piProviderPresets 对齐（自带
//! baseUrl/api/models，无需从 env 提取），选预设时直接填入 DSH 表单。

import type { PresetFamilyFields } from "./presetFamilies";
import type { ProviderCategory } from "@/types";
import type { PresetTheme } from "./claudeProviderPresets";

export type DshApiFormat =
  | "anthropic-messages"
  | "openai-completions"
  | "openai-responses";

export interface DshProviderPreset extends PresetFamilyFields {
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: {
    displayName?: string;
    api: DshApiFormat;
    baseUrl: string;
    models: Array<{ id: string; name: string }>;
  };
  category?: ProviderCategory;
  icon?: string;
  iconColor?: string;
  hidden?: boolean;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  theme?: PresetTheme;
}

export const dshProviderPresets: DshProviderPreset[] = [
  {
    name: "CC Switch Aggregate",
    websiteUrl: "http://127.0.0.1:15721/claude-desktop",
    settingsConfig: {
      displayName: "CC Switch Aggregate",
      api: "anthropic-messages",
      baseUrl: "http://127.0.0.1:15721/claude-desktop",
      models: [],
    },
    category: "aggregator",
    icon: "cc-switch",
  },
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    settingsConfig: {
      displayName: "DeepSeek",
      api: "openai-completions",
      baseUrl: "https://api.deepseek.com/v1",
      models: [
        { id: "deepseek-chat", name: "DeepSeek Chat" },
        { id: "deepseek-reasoner", name: "DeepSeek Reasoner" },
      ],
    },
    category: "third_party",
    icon: "deepseek",
  },
  {
    name: "Kimi",
    websiteUrl: "https://platform.moonshot.cn",
    settingsConfig: {
      displayName: "Kimi",
      api: "openai-completions",
      baseUrl: "https://api.moonshot.cn/v1",
      models: [
        { id: "kimi-k2.7-code", name: "Kimi K2.7 Code" },
        { id: "kimi-k2.5", name: "Kimi K2.5" },
      ],
    },
    category: "third_party",
    icon: "kimi",
  },
  {
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    settingsConfig: {
      displayName: "Zhipu GLM",
      api: "openai-completions",
      baseUrl: "https://open.bigmodel.cn/api/paas/v4",
      models: [
        { id: "glm-5.3", name: "GLM 5.3" },
        { id: "glm-5.3-flash", name: "GLM 5.3 Flash" },
      ],
    },
    category: "third_party",
    icon: "zhipu",
  },
];
