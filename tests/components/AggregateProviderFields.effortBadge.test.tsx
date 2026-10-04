import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AggregateProviderFields } from "@/components/providers/forms/AggregateProviderFields";
import type { AggregateRoutes, Provider } from "@/types";

function makeRoutes(
  pairs: Array<[string, "sonnet" | "haiku"]>,
): AggregateRoutes {
  return {
    slots: pairs.map(([routeId, tier], i) => ({
      routeId,
      tier,
      providerId: "p1",
      upstreamModel: `m${i}`,
      supports1m: false,
    })),
    defaultTarget: { kind: "providerId", value: "p1" },
  };
}

function renderEditor(value: AggregateRoutes) {
  return render(
    <AggregateProviderFields
      value={value}
      onChange={vi.fn()}
      candidates={[{ id: "p1", name: "P1" } as unknown as Provider]}
      modelsForProvider={() => []}
      fetchingProviderId={null}
      onFetchModels={vi.fn()}
    />,
  );
}

describe("聚合编辑器三态思考徽标", () => {
  it("展开卡后 ladder 槽显示 强度✓、溢出槽显示 ✗", async () => {
    const user = userEvent.setup();
    renderEditor(
      makeRoutes([
        ["claude-sonnet-5", "sonnet"],
        ["claude-haiku-3", "haiku"],
      ]),
    );
    await user.click(screen.getByRole("button", { expanded: false })); // 展开唯一的卡
    expect(screen.getByText("强度✓")).toBeInTheDocument();
    expect(screen.getByText("✗")).toBeInTheDocument();
  });

  it("extended 槽显示 思考开关", async () => {
    const user = userEvent.setup();
    renderEditor(makeRoutes([["claude-sonnet-4-5", "sonnet"]]));
    await user.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("思考开关")).toBeInTheDocument();
  });

  it("折叠摘要含强度计数", () => {
    renderEditor(
      makeRoutes([
        ["claude-sonnet-5", "sonnet"],
        ["claude-haiku-3", "haiku"],
      ]),
    );
    expect(screen.getByText("2 个模型 · 1 强度")).toBeInTheDocument();
  });
});

describe("聚合编辑器上限下拉", () => {
  it("上限下拉：ladder 槽可选，溢出槽禁用", async () => {
    // Radix Select 展开时会对首项调用 scrollIntoView（jsdom 未实现）
    Element.prototype.scrollIntoView = vi.fn();
    const user = userEvent.setup();
    const { container } = renderEditor(
      makeRoutes([
        ["claude-sonnet-5", "sonnet"],
        ["claude-haiku-3", "haiku"],
      ]),
    );
    await user.click(screen.getByRole("button", { expanded: false }));
    // 槽位行带 routeId title，行内只有上限下拉这一个 combobox，
    // 故按行取即得「该槽的上限触发器」（页面上还有兜底目标/默认模型/卡头下拉）
    const capTriggerOf = (routeId: string) =>
      within(
        container.querySelector(`[title="${routeId}"]`) as HTMLElement,
      ).getByRole("combobox");
    expect(capTriggerOf("claude-sonnet-5")).toBeEnabled();
    expect(capTriggerOf("claude-haiku-3")).toBeDisabled();

    // 选项来自该 ID 的实际阶梯（claude-sonnet-5 为 low…max 五档）
    await user.click(capTriggerOf("claude-sonnet-5"));
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      "不限制",
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
    ]);
  });

  it("普通提交保留每个既有模型的 ID 和有效强度上限", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    // 两张卡各一条 sonnet 行：重编号后 p1 取池内第 1 位（claude-sonnet-4-6，ladder），
    // p2 落到第 2 位（claude-sonnet-4-5，extended）。p2 的输入 ID claude-sonnet-5
    // 本身是 ladder——若在 assignSlotIds 之前判定，就会漏剥它的 maxEffort。
    render(
      <AggregateProviderFields
        value={{
          slots: [
            {
              routeId: "claude-sonnet-4-6",
              tier: "sonnet",
              providerId: "p1",
              upstreamModel: "m1",
              supports1m: false,
              maxEffort: "max",
            },
            {
              routeId: "claude-sonnet-5",
              tier: "sonnet",
              providerId: "p2",
              upstreamModel: "m2",
              supports1m: false,
              maxEffort: "xhigh",
            },
          ],
          defaultTarget: { kind: "providerId", value: "p1" },
        }}
        onChange={onChange}
        candidates={[
          { id: "p1", name: "P1" } as unknown as Provider,
          { id: "p2", name: "P2" } as unknown as Provider,
        ]}
        modelsForProvider={() => []}
        fetchingProviderId={null}
        onFetchModels={vi.fn()}
      />,
    );
    await user.click(screen.getAllByRole("button", { expanded: false })[0]);
    await user.click(screen.getAllByRole("switch")[0]); // 任意一次提交

    const next = onChange.mock.calls.at(-1)?.[0] as AggregateRoutes;
    expect(next.slots.map((s) => [s.routeId, s.maxEffort])).toEqual([
      ["claude-sonnet-4-6", "max"], // ladder：保留
      ["claude-sonnet-5", "xhigh"], // ID 和能力均保留
    ]);
  });

  it("删除其他模型不收窄剩余模型的 ID 或强度上限", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    // 存量数据：p1 槽存的是 sonnet-5 + xhigh（删槽轮换前的旧编号）。提交必先
    // assignSlotIds 重编号——删掉 p2 的槽后，p1 槽按序号 1 重算为 claude-sonnet-4-6，
    // 同为「完整阶梯」但阶梯不含 xhigh。只判三态的实现会把它留成隐形残留
    // （Radix Select 值不在选项中只显 placeholder，用户看不见）。
    render(
      <AggregateProviderFields
        value={{
          slots: [
            {
              routeId: "claude-sonnet-5",
              tier: "sonnet",
              providerId: "p1",
              upstreamModel: "m1",
              supports1m: false,
              maxEffort: "xhigh",
            },
            {
              routeId: "claude-sonnet-4-5",
              tier: "sonnet",
              providerId: "p2",
              upstreamModel: "m2",
              supports1m: false,
            },
          ],
          defaultTarget: { kind: "providerId", value: "p1" },
        }}
        onChange={onChange}
        candidates={[
          { id: "p1", name: "P1" } as unknown as Provider,
          { id: "p2", name: "P2" } as unknown as Provider,
        ]}
        modelsForProvider={() => []}
        fetchingProviderId={null}
        onFetchModels={vi.fn()}
      />,
    );
    // 展开第二张卡，删掉它的槽 → 触发重编号
    await user.click(screen.getAllByRole("button", { expanded: false })[1]);
    await user.click(screen.getByTitle("删除该模型"));

    const next = onChange.mock.calls.at(-1)?.[0] as AggregateRoutes;
    expect(next.slots.map((s) => [s.routeId, s.maxEffort])).toEqual([
      ["claude-sonnet-5", "xhigh"],
    ]);
  });
});
