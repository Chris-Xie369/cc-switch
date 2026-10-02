import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { CodexAggregateFields } from "@/components/providers/forms/CodexAggregateFields";
import {
  ProviderForm,
  type ProviderFormValues,
} from "@/components/providers/forms/ProviderForm";
import type { CodexAggregateRoutes, Provider, ProviderMeta } from "@/types";
import type { FetchedModel } from "@/lib/api/model-fetch";
import { createTestQueryClient } from "../utils/testQueryClient";

// Radix Select 展开时会对首项调用 scrollIntoView（jsdom 未实现）
Element.prototype.scrollIntoView = vi.fn();

const p1 = { id: "p1", name: "Kimi" } as unknown as Provider;
const p2 = { id: "p2", name: "Ark" } as unknown as Provider;

// 模型名与上游模型刻意取不同值：两者相同的槽位会让 getByDisplayValue
// 命中两个输入框，测试无法定位到具体一列。
const baseValue: CodexAggregateRoutes = {
  slots: [
    { model: "fast", providerId: "p1", upstreamModel: "kimi-k2" },
    { model: "smart", providerId: "p1", upstreamModel: "glm-5.3" },
  ],
  defaultTarget: { kind: "providerId", value: "p1" },
};

// 编辑器 props 里除 value/onChange 外的四项在下面都有默认值，故全部可选化。
type EditorProps = Partial<
  Omit<React.ComponentProps<typeof CodexAggregateFields>, "value" | "onChange">
> & {
  initial?: CodexAggregateRoutes;
  onChange?: (next: CodexAggregateRoutes) => void;
};

/** 受控外壳：真实使用时父组件回灌值，逐字输入才不会被 React 重置。 */
function ControlledEditor({
  initial = baseValue,
  onChange,
  ...rest
}: EditorProps) {
  const [value, setValue] = useState<CodexAggregateRoutes>(initial);
  return (
    <CodexAggregateFields
      value={value}
      onChange={(next) => {
        onChange?.(next);
        setValue(next);
      }}
      candidates={[p1, p2]}
      modelsForProvider={() => []}
      fetchingProviderId={null}
      onFetchModels={vi.fn()}
      {...rest}
    />
  );
}

/** 取最后一次 onChange 的入参（提交形状断言的统一入口）。 */
function lastChange(mock: { mock: { calls: unknown[][] } }) {
  return mock.mock.calls.at(-1)![0] as CodexAggregateRoutes;
}

describe("Codex 聚合编辑器：行增删与提交形状", () => {
  it("初始渲染既有槽位行（每行四个字段）", () => {
    render(<ControlledEditor />);
    expect(screen.getByDisplayValue("fast")).toBeInTheDocument();
    expect(screen.getByDisplayValue("kimi-k2")).toBeInTheDocument();
    expect(screen.getByDisplayValue("smart")).toBeInTheDocument();
    expect(screen.getByDisplayValue("glm-5.3")).toBeInTheDocument();
    // 两个行尾删除按钮
    expect(screen.getAllByRole("button", { name: "删除该模型" })).toHaveLength(
      2,
    );
  });

  it("空表显示空态，添加行产生空槽位", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ControlledEditor
        initial={{
          slots: [],
          defaultTarget: { kind: "providerId", value: "" },
        }}
        onChange={onChange}
      />,
    );
    expect(
      screen.getByText("还没有槽位，点击下方「添加模型」开始配置。"),
    ).toBeInTheDocument();

    // 既有「模型映射」高级区也有同名按钮；聚合编辑器渲染在最顶部，取首个
    await user.click(screen.getAllByRole("button", { name: "添加模型" })[0]);
    const next = lastChange(onChange);
    expect(next.slots).toEqual([
      { model: "", providerId: "", upstreamModel: "" },
    ]);
  });

  it("删除行只保留其余槽位（按索引而非标识删）", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<ControlledEditor onChange={onChange} />);

    await user.click(screen.getAllByRole("button", { name: "删除该模型" })[0]);
    expect(lastChange(onChange).slots.map((s) => s.model)).toEqual(["smart"]);
  });

  it("编辑模型名 / 上游模型保留同行的目标供应商", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<ControlledEditor onChange={onChange} />);

    const modelInputs = [
      screen.getByDisplayValue("fast"),
      screen.getByDisplayValue("smart"),
    ];
    await user.clear(modelInputs[1]);
    await user.type(modelInputs[1], "x");
    expect(lastChange(onChange).slots).toEqual([
      { model: "fast", providerId: "p1", upstreamModel: "kimi-k2" },
      { model: "x", providerId: "p1", upstreamModel: "glm-5.3" },
    ]);
  });

  it("显示名留空提交时省略 label 键（而非空串）", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ControlledEditor
        initial={{
          slots: [
            { model: "fast", providerId: "p1", upstreamModel: "kimi-k2" },
            {
              model: "smart",
              providerId: "p1",
              upstreamModel: "glm-5.3",
              // 纯空白：后端写模型目录时 trim 后丢弃，占位也必须回落
              label: "   ",
            },
          ],
          defaultTarget: { kind: "providerId", value: "p1" },
        }}
        onChange={onChange}
      />,
    );

    // 显示名输入框（空白 label 按未设置处理，占位回落成「Kimi · glm-5.3」）
    const labelInput = screen.getByPlaceholderText("Kimi · glm-5.3");
    await user.clear(labelInput);
    await user.type(labelInput, "智谱");
    expect(lastChange(onChange).slots[1].label).toBe("智谱");

    // 清空后回到未设置：编辑器把 label 落成 undefined
    await user.clear(labelInput);
    expect(lastChange(onChange).slots[1].label).toBeUndefined();
  });

  it("兜底目标可选槽位（slotId），默认模型可设为槽位 model", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<ControlledEditor onChange={onChange} />);

    // 兜底目标 / 默认模型 是页面上前两个 combobox（行内目标供应商在后面）
    const triggers = screen.getAllByRole("combobox");
    await user.click(triggers[0]);
    const slotOption = (await screen.findAllByRole("option")).find((o) =>
      o.textContent?.includes("(fast)"),
    );
    await user.click(slotOption!);
    expect(lastChange(onChange).defaultTarget).toEqual({
      kind: "slotId",
      value: "fast",
    });

    await user.click(screen.getAllByRole("combobox")[1]);
    const modelOptions = await screen.findAllByRole("option");
    await user.click(modelOptions.find((o) => o.textContent === "smart")!);
    expect(lastChange(onChange).defaultModel).toBe("smart");
  });

  it("默认模型可回到未设置（提交时省略 defaultModel 键）", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ControlledEditor
        initial={{ ...baseValue, defaultModel: "fast" }}
        onChange={onChange}
      />,
    );
    await user.click(screen.getAllByRole("combobox")[1]);
    const unset = (await screen.findAllByRole("option")).find((o) =>
      o.textContent?.includes("未设置"),
    );
    await user.click(unset!);
    const next = lastChange(onChange);
    expect(next.defaultModel).toBeUndefined();
    // 序列化后不落键（后端 serde 的 defaultModel 是 Option）
    expect("defaultModel" in JSON.parse(JSON.stringify(next))).toBe(false);
  });

  it("行内目标供应商只列传入的候选；存量引用被过滤掉时保底显示不空", async () => {
    const user = userEvent.setup();
    // 候选里没有 p1（被过滤掉 = 已被删除 / 被改造成聚合）：仍要能选中，
    // 否则选择器空白会让用户以为没配置、也无法看清坏在哪一行。
    render(<ControlledEditor candidates={[p2]} />);
    // 行内目标供应商触发器：兜底目标 / 默认模型 之后的第一、二个
    await user.click(screen.getAllByRole("combobox")[2]);
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["Ark", "p1"]);
  });
});

describe("Codex 聚合编辑器：模型列表按供应商 id 缓存", () => {
  it("同一供应商的多行共用一份缓存，删行不触发重拉也不错位", async () => {
    const user = userEvent.setup();
    const onFetchModels = vi.fn();
    const models: Record<string, FetchedModel[]> = {
      p1: [{ id: "kimi-k2", ownedBy: "moonshot" }],
    };
    render(
      <ControlledEditor
        modelsForProvider={(id) => models[id] ?? []}
        onFetchModels={onFetchModels}
      />,
    );

    // 两行都指向 p1 → 两个模型下拉都出现（缓存按 id 命中，与行索引无关）
    const dropdowns = () =>
      screen.queryAllByRole("button", { name: "Select model" });
    expect(dropdowns()).toHaveLength(2);

    // 切到 p2 的行：p2 没有缓存 → 不出现下拉，恰好证明缓存不是按行索引
    await user.click(screen.getAllByRole("combobox")[3]);
    await user.click(
      (await screen.findAllByRole("option")).find(
        (o) => o.textContent === "Ark",
      )!,
    );
    // 现在第 2 行指向 p2：整页只剩 1 个下拉（第一行的 p1 仍命中缓存）
    await waitFor(() => expect(dropdowns()).toHaveLength(1));

    // 删掉指向 p1 的行：p2 行没有缓存 → 0 个下拉，且不发生任何拉取
    await user.click(screen.getAllByRole("button", { name: "删除该模型" })[0]);
    await waitFor(() => expect(dropdowns()).toHaveLength(0));
    expect(onFetchModels).not.toHaveBeenCalled();
  });

  it("拉取按钮按目标供应商 id 上报（父组件据此缓存）", async () => {
    const user = userEvent.setup();
    const onFetchModels = vi.fn();
    render(<ControlledEditor onFetchModels={onFetchModels} />);

    await user.click(screen.getAllByTitle("获取模型列表")[1]);
    expect(onFetchModels).toHaveBeenCalledTimes(1);
    expect(onFetchModels.mock.calls[0][0]).toMatchObject({ id: "p1" });
  });

  it("目标供应商未选时拉取按钮禁用", () => {
    render(
      <ControlledEditor
        initial={{
          slots: [{ model: "gpt-5.1", providerId: "", upstreamModel: "" }],
          defaultTarget: { kind: "providerId", value: "" },
        }}
      />,
    );
    expect(screen.getByTitle("获取模型列表")).toBeDisabled();
  });
});

describe("Codex 聚合编辑器：提交形状（编辑器层）", () => {
  it("槽位形状只有四个键，不夹带 Claude 侧的档位字段", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ControlledEditor
        initial={{
          slots: [
            {
              model: "fast",
              providerId: "p1",
              upstreamModel: "kimi-k2",
              label: "Kimi K2",
            },
          ],
          defaultTarget: { kind: "slotId", value: "fast" },
          defaultModel: "fast",
        }}
        onChange={onChange}
      />,
    );
    await user.type(screen.getByDisplayValue("Kimi K2"), "!");
    const next = lastChange(onChange);
    // 与后端 serde（camelCase，label 为 Option）逐字对应
    expect(Object.keys(next.slots[0]).sort()).toEqual([
      "label",
      "model",
      "providerId",
      "upstreamModel",
    ]);
    expect(next.slots[0].label).toBe("Kimi K2!");
    expect(next.defaultTarget).toEqual({ kind: "slotId", value: "fast" });
    expect(next.defaultModel).toBe("fast");
  });
});

// ---------------------------------------------------------------------------
// 表单层（ProviderForm）：开关接线 + meta 提交形状。
// 编辑器与提交都在 ProviderForm 里（CodexFormFields 只渲染 UI），故 meta
// 的写入/删除必须在这一层验证——单元测试编辑器证明不了 key 落不落库。
// ---------------------------------------------------------------------------

const toastMocks = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn() }));

vi.mock("sonner", () => ({
  toast: {
    error: toastMocks.error,
    success: toastMocks.success,
    info: vi.fn(),
  },
}));

vi.mock("@/components/providers/forms/CodexConfigEditor", () => ({
  // 用 hideForAggregate 当门闸渲染出 testid：配置块藏起时它一并消失，
  // 便于断言「聚合开启 → 端点/凭据/通用配置整块不再暴露」。
  default: ({ hideForAggregate }: { hideForAggregate?: boolean }) =>
    hideForAggregate ? null : <div data-testid="codex-config-editor" />,
}));

vi.mock("@/components/providers/forms/ProviderAdvancedConfig", () => ({
  ProviderAdvancedConfig: () => <div data-testid="advanced-config" />,
}));

vi.mock("@/components/providers/forms/hooks", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/components/providers/forms/hooks")>();
  return {
    ...actual,
    useCopilotAuth: () => ({ isAuthenticated: false, accounts: [] }),
    useCodexOauth: () => ({
      isAuthenticated: false,
      defaultAccountId: null,
      accounts: [],
    }),
    useXaiOauth: () => ({ isAuthenticated: false, accounts: [] }),
  };
});

vi.mock("@/lib/query", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/query")>();
  return {
    ...actual,
    useSettingsQuery: () => ({ data: { commonConfigConfirmed: true } }),
  };
});

function renderCodexProviderForm(opts: {
  onSubmit: (values: ProviderFormValues) => void;
  initialMeta?: ProviderMeta;
  category?: "custom" | "official" | "third_party";
}) {
  const queryClient = createTestQueryClient();
  return render(
    <QueryClientProvider client={queryClient}>
      <ProviderForm
        appId="codex"
        submitLabel="save-provider"
        onSubmit={opts.onSubmit}
        onCancel={vi.fn()}
        initialData={{
          name: "Agg",
          category: opts.category ?? "custom",
          // 带端点 + Key：否则非官方卡片的软校验会先弹「仍要保存？」确认框
          settingsConfig: {
            auth: { OPENAI_API_KEY: "sk-x" },
            config:
              'model_provider = "p"\n[model_providers.p]\nbase_url = "https://api.example/v1"\nwire_api = "responses"',
          },
          meta: opts.initialMeta,
        }}
      />
    </QueryClientProvider>,
  );
}

describe("Codex 聚合：开关接线与 meta 提交", () => {
  it("普通非官方卡片可达开关；开启后保存写出 codexAggregateRoutes", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    renderCodexProviderForm({ onSubmit });

    const toggle = screen.getByRole("switch", { name: "启用聚合路由" });
    expect(toggle).toHaveAttribute("data-state", "unchecked");

    await user.click(toggle);
    // 开启即初始化空表（空槽位是合法中间态，硬门禁在后端）
    // 既有「模型映射」高级区也有同名按钮；聚合编辑器渲染在最顶部，取首个
    await user.click(screen.getAllByRole("button", { name: "添加模型" })[0]);
    await user.type(
      screen.getAllByPlaceholderText("客户端模型名")[0],
      "gpt-5.1",
    );

    await user.click(screen.getByRole("button", { name: "save-provider" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));

    const meta = onSubmit.mock.calls[0][0].meta!;
    // 兜底目标未选 → 空串，后端会以 toast 拒（UI 不硬拦，与 Claude 侧一致）
    expect(meta.codexAggregateRoutes).toEqual({
      slots: [{ model: "gpt-5.1", providerId: "", upstreamModel: "" }],
      defaultTarget: { kind: "providerId", value: "" },
    });
    // defaultModel 未设置 → 序列化时不落键（后端 Option 为 None）。
    // 断言打在 JSON 上而非 JS 对象：真正发给后端的是序列化结果。
    const json = JSON.parse(JSON.stringify(meta.codexAggregateRoutes));
    expect("defaultModel" in json).toBe(false);
  });

  it("关闭开关后保存删除 meta 键（普通供应商 JSON 不变）", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    renderCodexProviderForm({
      onSubmit,
      initialMeta: {
        codexAggregateRoutes: {
          slots: [
            { model: "gpt-5.1", providerId: "p1", upstreamModel: "kimi-k2" },
          ],
          defaultTarget: { kind: "providerId", value: "p1" },
        },
      },
    });

    // 带表打开 → 开关为开、编辑器可见
    const toggle = screen.getByRole("switch", { name: "启用聚合路由" });
    expect(toggle).toHaveAttribute("data-state", "checked");
    expect(screen.getByDisplayValue("gpt-5.1")).toBeInTheDocument();

    await user.click(toggle);
    await user.click(screen.getByRole("button", { name: "save-provider" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));

    const meta = onSubmit.mock.calls[0][0].meta!;
    expect("codexAggregateRoutes" in meta).toBe(false);
  });

  it("已有表 + 改动显示名后保存：label 保留，槽位形状与后端 serde 一致", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    renderCodexProviderForm({
      onSubmit,
      initialMeta: {
        codexAggregateRoutes: {
          slots: [
            { model: "gpt-5.1", providerId: "p1", upstreamModel: "kimi-k2" },
            {
              model: "glm-5.3",
              providerId: "p1",
              upstreamModel: "glm-5.3",
              label: "旧名",
            },
          ],
          defaultTarget: { kind: "providerId", value: "p1" },
          defaultModel: "gpt-5.1",
        },
      },
    });

    await user.click(screen.getByRole("button", { name: "save-provider" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));

    const routes = (onSubmit.mock.calls[0][0].meta as Record<string, unknown>)
      .codexAggregateRoutes as Record<string, unknown>;
    expect(routes).toEqual({
      slots: [
        { model: "gpt-5.1", providerId: "p1", upstreamModel: "kimi-k2" },
        {
          model: "glm-5.3",
          providerId: "p1",
          upstreamModel: "glm-5.3",
          label: "旧名",
        },
      ],
      defaultTarget: { kind: "providerId", value: "p1" },
      defaultModel: "gpt-5.1",
    });
    // 未填 label 的槽位不带该键（后端 skip_serializing_if 会省略）
    expect("label" in (routes.slots as Record<string, unknown>[])[0]).toBe(
      false,
    );
  });

  it("官方 Codex 卡片不渲染聚合开关", () => {
    renderCodexProviderForm({ onSubmit: vi.fn(), category: "official" });
    expect(screen.queryByRole("switch", { name: "启用聚合路由" })).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// 聚合开启时的表单门禁：聚合卡片无端点无凭据，写 live 时由后端
// `apply_codex_aggregate_seed` 合成整份配置顶替用户输入。渲染出来的端点 / Key /
// 通用配置开关全是「改了看不到效果」的假控件，软校验还会为它们弹「仍要保存？」。
// 判据与 Claude Desktop 侧聚合一致：开启即豁免、开启即藏起来。
// ---------------------------------------------------------------------------

describe("Codex 聚合：表单门禁（藏端点/凭据/配置块 + 豁免软校验）", () => {
  const routes: CodexAggregateRoutes = {
    slots: [{ model: "gpt-5.1", providerId: "p1", upstreamModel: "kimi-k2" }],
    defaultTarget: { kind: "providerId", value: "p1" },
  };

  it("聚合关闭时端点 / Key / 配置块照常渲染", () => {
    const { container } = renderCodexProviderForm({ onSubmit: vi.fn() });

    // i18n 在测试里回落成 key，按稳定的元素 id 定位（label 文案会随 locale 变）
    expect(container.querySelector("#codexApiKey")).not.toBeNull();
    expect(container.querySelector("#codexBaseUrl")).not.toBeNull();
    expect(screen.getByTestId("codex-config-editor")).toBeInTheDocument();
  });

  it("聚合开启后端点 / Key / 配置块一并消失", () => {
    const { container } = renderCodexProviderForm({
      onSubmit: vi.fn(),
      initialMeta: { codexAggregateRoutes: routes },
    });

    expect(container.querySelector("#codexApiKey")).toBeNull();
    expect(container.querySelector("#codexBaseUrl")).toBeNull();
    expect(screen.queryByTestId("codex-config-editor")).toBeNull();
  });

  it("存量聚合卡片（无端点无 Key）保存不弹「仍要保存？」确认框", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    // settingsConfig 为空对象：聚合卡片本就没有端点与凭据
    render(
      <QueryClientProvider client={createTestQueryClient()}>
        <ProviderForm
          appId="codex"
          submitLabel="save-provider"
          onSubmit={onSubmit}
          onCancel={vi.fn()}
          initialData={{
            name: "Agg",
            category: "custom",
            settingsConfig: { auth: {}, config: "" },
            meta: { codexAggregateRoutes: routes },
          }}
        />
      </QueryClientProvider>,
    );

    await user.click(screen.getByRole("button", { name: "save-provider" }));

    // 软校验豁免 → 直接提交，没有确认框拦截
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(screen.queryByText("仍要保存")).toBeNull();
  });

  it("非聚合卡片缺端点时仍弹确认框（豁免不外溢）", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <QueryClientProvider client={createTestQueryClient()}>
        <ProviderForm
          appId="codex"
          submitLabel="save-provider"
          onSubmit={onSubmit}
          onCancel={vi.fn()}
          initialData={{
            name: "Plain",
            category: "custom",
            settingsConfig: { auth: {}, config: "" },
          }}
        />
      </QueryClientProvider>,
    );

    await user.click(screen.getByRole("button", { name: "save-provider" }));

    expect(await screen.findByText("仍要保存")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
