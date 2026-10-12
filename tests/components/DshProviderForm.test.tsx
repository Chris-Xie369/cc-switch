import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { DshProviderForm } from "@/components/providers/forms/DshProviderForm";
import { dshProviderPresets } from "@/config/dshProviderPresets";

// 预设选择器走 portal；测试只验「预设数据 → 表单状态」链路——直接渲染表单，
// 模拟 selectPreset 的效果由表单自己完成（点选预设不在本测试范围）。

// 其实更直接：验证 dshProviderPresets 的 DeepSeek 模型带元数据 + ModelRow 展开渲染
describe("DshProviderForm metadata chain", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("preset derivation produces full metadata for DeepSeek", () => {
    const ds = dshProviderPresets.find((p) => p.name === "DeepSeek");
    expect(ds).toBeTruthy();
    const m = ds!.settingsConfig.models[0];
    expect(m.contextWindow).toBe(1000000);
    expect(m.maxTokens).toBe(384000);
    expect(m.reasoning).toBe(true);
    expect(m.input).toContain("text");
  });

  it("renders model rows with chevron expand buttons", async () => {
    const ds = dshProviderPresets.find((p) => p.name === "DeepSeek")!;
    const initialData = {
      name: "DeepSeek",
      settingsConfig: {
        displayName: "DeepSeek",
        api: "openai-completions",
        baseUrl: "https://api.deepseek.com/v1",
        apiKey: "",
        models: ds.settingsConfig.models.map((m) => ({ ...m })),
      },
    };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
      <DshProviderForm
        appId="deepseek-harness"
        submitLabel="Add"
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
        initialData={initialData}
      />,
      </QueryClientProvider>,
    );
    // 模型行应有展开按钮
    const buttons = screen.getAllByRole("button");
    const chevronButtons = buttons.filter((b) =>
      b.getAttribute("aria-label")?.includes("deepseekHarness.form.modelMeta"),
    );
    expect(chevronButtons.length).toBeGreaterThan(0);
    // 点第一个展开
    await userEvent.click(chevronButtons[0]);
    // 展开后应有 contextWindow 输入框且带值
    const cwInputs = screen.getAllByRole("spinbutton");
    const cw = cwInputs.find((i) => (i as HTMLInputElement).value === "1000000");
    expect(cw).toBeTruthy();
  });
});
