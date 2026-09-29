import { render, screen } from "@testing-library/react";
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
