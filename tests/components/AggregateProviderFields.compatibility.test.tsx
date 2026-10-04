import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AggregateProviderFields } from "@/components/providers/forms/AggregateProviderFields";
import type { AggregateRoutes, Provider } from "@/types";

const initial: AggregateRoutes = {
  slots: [
    {
      routeId: "claude-sonnet-4-6",
      tier: "sonnet",
      providerId: "a",
      upstreamModel: "upstream-a",
      label: "A",
    },
    {
      routeId: "claude-sonnet-4-5",
      tier: "sonnet",
      providerId: "a",
      upstreamModel: "upstream-b",
      label: "B",
    },
  ],
  defaultTarget: { kind: "slotId", value: "claude-sonnet-4-6" },
  defaultModel: "claude-sonnet-4-6",
};

async function setup() {
  const change = vi.fn();
  function Controlled() {
    const [value, setValue] = useState(initial);
    return (
      <AggregateProviderFields
        value={value}
        onChange={(next) => {
          change(next);
          setValue(next);
        }}
        candidates={[
          { id: "a", name: "Synthetic A", settingsConfig: {} } as Provider,
        ]}
        modelsForProvider={() => []}
        fetchingProviderId={null}
        onFetchModels={() => {}}
      />
    );
  }
  render(<Controlled />);
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "全部展开" }));
  return change;
}

describe("lossless multi-model editing", () => {
  it("shows both existing models in the same provider/tier", async () => {
    await setup();
    expect(screen.getByDisplayValue("upstream-a")).toBeInTheDocument();
    expect(screen.getByDisplayValue("upstream-b")).toBeInTheDocument();
  });

  it("editing one model keeps the other and preserves identities", async () => {
    const change = await setup();
    fireEvent.change(screen.getByDisplayValue("upstream-a"), {
      target: { value: "changed-a" },
    });
    const next = change.mock.lastCall?.[0] as AggregateRoutes;
    expect(next.slots).toHaveLength(2);
    expect(next.slots[1]).toEqual(initial.slots[1]);
    expect(next.defaultModel).toBe(initial.defaultModel);
  });

  it("deleting one row leaves the second model available", async () => {
    const change = await setup();
    await userEvent
      .setup()
      .click(screen.getAllByRole("button", { name: "删除该模型" })[0]);
    const next = change.mock.lastCall?.[0] as AggregateRoutes;
    expect(next.slots).toEqual([initial.slots[1]]);
    expect(screen.getByDisplayValue("upstream-b")).toBeInTheDocument();
  });
});
