import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { AggregateProviderFields } from "@/components/providers/forms/AggregateProviderFields";
import type { AggregateRoutes, Provider } from "@/types";

it("changing the source after a migration preview invalidates confirmation", async () => {
  const initial: AggregateRoutes = {
    slots: [
      {
        routeId: "claude-fable-glm",
        tier: "fable",
        providerId: "a",
        upstreamModel: "old",
      },
    ],
    defaultTarget: { kind: "providerId", value: "a" },
  };
  const common = {
    onChange: vi.fn(),
    candidates: [{ id: "a", name: "A", settingsConfig: {} } as Provider],
    modelsForProvider: () => [],
    fetchingProviderId: null,
    onFetchModels: () => {},
  };
  const { rerender } = render(
    <AggregateProviderFields {...common} value={initial} />,
  );
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "预览旧路由转换" }));
  expect(
    screen.getByRole("button", { name: "确认转换路由 ID" }),
  ).toBeInTheDocument();
  rerender(
    <AggregateProviderFields
      {...common}
      value={{
        ...initial,
        slots: [{ ...initial.slots[0], upstreamModel: "new-edit" }],
      }}
    />,
  );
  expect(
    screen.queryByRole("button", { name: "确认转换路由 ID" }),
  ).not.toBeInTheDocument();
  expect(common.onChange).not.toHaveBeenCalled();
});

it("legacy invalid IDs change only after explicit preview, backup and confirmation", async () => {
  const oldId = "claude-fable-zhipu-glm";
  const routes: AggregateRoutes = {
    slots: [
      {
        routeId: oldId,
        tier: "fable",
        providerId: "a",
        upstreamModel: "model-a",
      },
    ],
    defaultTarget: { kind: "slotId", value: oldId },
    defaultModel: oldId,
    aliasRules: [{ prefix: "team", slotId: oldId }],
  };
  const change = vi.fn();
  render(
    <AggregateProviderFields
      value={routes}
      onChange={change}
      candidates={[{ id: "a", name: "A", settingsConfig: {} } as Provider]}
      modelsForProvider={() => []}
      fetchingProviderId={null}
      onFetchModels={() => {}}
    />,
  );
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "预览旧路由转换" }));
  expect(change).not.toHaveBeenCalled();
  expect(
    screen.getByRole("button", { name: "确认转换路由 ID" }),
  ).toBeDisabled();
  class MockURL extends URL {
    static createObjectURL = vi.fn(() => "blob:synthetic");
    static revokeObjectURL = vi.fn();
  }
  vi.stubGlobal("URL", MockURL);
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => {});
  await user.click(screen.getByRole("button", { name: "下载路由备份" }));
  await user.click(screen.getByRole("button", { name: "确认转换路由 ID" }));
  const next = change.mock.lastCall?.[0] as AggregateRoutes;
  expect(next.slots[0].routeId).not.toBe(oldId);
  expect(next.slots[0].upstreamModel).toBe("model-a");
  expect(next.defaultTarget.value).toBe(next.slots[0].routeId);
  expect(next.defaultModel).toBe(next.slots[0].routeId);
  expect(next.aliasRules?.[0].slotId).toBe(next.slots[0].routeId);
  click.mockRestore();
  vi.unstubAllGlobals();
});
