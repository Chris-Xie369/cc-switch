import { useState } from "react";
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

  it("编辑前缀回调 onChange 且带上新前缀", async () => {
    const user = userEvent.setup();
    // 受控组件：值必须回灌，否则 clear 之后 React 把 DOM 重置回旧前缀，
    // 逐字输入会追加在旧值后面（真实使用中父组件正是回灌的）
    const Controlled = () => {
      const [current, setCurrent] = useState(value);
      return (
        <AggregateProviderFields
          {...props}
          value={current}
          onChange={(v) => {
            props.onChange(v);
            setCurrent(v);
          }}
        />
      );
    };
    render(<Controlled />);
    const input = screen.getByDisplayValue("claude-sonnet");
    await user.clear(input);
    await user.type(input, "claude-opus");
    const calls = props.onChange.mock.calls;
    const last = calls[calls.length - 1][0] as AggregateRoutes;
    // 槽位 ID 由 commit 内的 assignSlotIds 重编号（sonnet 首位取池内真 ID）
    expect(last.aliasRules?.[0]).toEqual({
      prefix: "claude-opus",
      slotId: "claude-sonnet-4-6",
    });
    // 第二条规则不受影响（patch 只改被编辑的行）
    expect(last.aliasRules?.[1]).toEqual({ prefix: "claude-", slotId: "" });
  });
});
