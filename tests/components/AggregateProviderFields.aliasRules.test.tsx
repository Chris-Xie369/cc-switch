import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AggregateProviderFields } from "@/components/providers/forms/AggregateProviderFields";
import type { AggregateRoutes, Provider } from "@/types";

const value: AggregateRoutes = {
  slots: [
    {
      routeId: "claude-sonnet-4",
      tier: "sonnet",
      providerId: "p1",
      upstreamModel: "space-bunny-free",
      label: "OC · space-bunny-free",
      supports1m: false,
    },
  ],
  defaultTarget: { kind: "providerId", value: "p1" },
  aliasRules: [
    { prefix: "claude-sonnet", slotId: "claude-sonnet-4" },
    { prefix: "claude-", slotId: "" },
  ],
};

const props = {
  value,
  onChange: vi.fn(),
  candidates: [{ id: "p1", name: "OC" } as unknown as Provider],
  modelsForProvider: () => [],
  fetchingProviderId: null,
  onFetchModels: vi.fn(),
};

describe("别名路由区块", () => {
  it("渲染既有规则行（含悬空行），悬空行显示占位而非崩", () => {
    render(<AggregateProviderFields {...props} />);
    const prefixInput = screen.getByDisplayValue("claude-sonnet");
    expect(prefixInput).toBeInTheDocument();
    // 第二行 slotId 为空 → Select 显示占位（存在第二个 combobox 即可）
    expect(screen.getAllByRole("combobox").length).toBeGreaterThanOrEqual(2);
  });

  it("添加规则产生空行并回调 onChange", async () => {
    const user = userEvent.setup();
    render(<AggregateProviderFields {...props} />);
    await user.click(screen.getByRole("button", { name: "添加规则" }));
    expect(props.onChange).toHaveBeenCalled();
    const calls = props.onChange.mock.calls;
    const last = calls[calls.length - 1][0] as AggregateRoutes;
    expect(last.aliasRules).toHaveLength(3);
    expect(last.aliasRules?.[2]).toEqual({ prefix: "", slotId: "" });
  });

  it("删除规则行回调 onChange 且不残留", async () => {
    const user = userEvent.setup();
    render(<AggregateProviderFields {...props} />);
    const dels = screen.getAllByRole("button", { name: "删除该规则" });
    await user.click(dels[0]);
    const calls = props.onChange.mock.calls;
    const last = calls[calls.length - 1][0] as AggregateRoutes;
    expect(last.aliasRules).toEqual([{ prefix: "claude-", slotId: "" }]);
  });
});
