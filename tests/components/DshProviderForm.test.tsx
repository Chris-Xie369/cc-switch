import { describe, expect, it } from "vitest";
import { dshProviderPresets } from "@/config/dshProviderPresets";

describe("dsh preset derivation (Mcode-format)", () => {
  it("DeepSeek models carry full metadata in nested OpenCode format", () => {
    const ds = dshProviderPresets.find((p) => p.name === "DeepSeek");
    expect(ds).toBeTruthy();
    const models = ds!.settingsConfig.models;
    expect(Object.keys(models)).toContain("deepseek-v4-pro");
    const m = models["deepseek-v4-pro"];
    expect(m).toBeTruthy();
    expect(m!.limit?.context).toBe(1000000);
    expect(m!.limit?.output).toBe(384000);
    expect(m!.reasoning).toBe(true);
    expect((m!.modalities as Record<string, string[]>)?.input).toContain(
      "text",
    );
    expect((m!.modalities as Record<string, string[]>)?.output).toContain(
      "text",
    );
    expect(m!.name).toBe("DeepSeek V4 Pro");
  });

  it("config structure matches Mcode (options.baseURL + models map)", () => {
    const ds = dshProviderPresets.find((p) => p.name === "DeepSeek")!;
    const cfg = ds.settingsConfig;
    expect(cfg.options.baseURL).toBe("https://api.deepseek.com/v1");
    expect(cfg.api).toBe("openai-completions");
    expect(cfg.kind).toBe("custom");
    expect(cfg.enabled).toBe(true);
    expect(Object.keys(cfg.models).length).toBeGreaterThan(0);
  });

  it("derives from all Pi presets with supported protocols (60+ vendors)", () => {
    expect(dshProviderPresets.length).toBeGreaterThan(30);
  });
});
