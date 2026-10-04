import { describe, expect, it } from "vitest";
import {
  assignSlotIds,
  flattenProviderGroups,
  groupSlotsByProvider,
  migrateIncompatibleRouteIds,
  needsRouteIdMigration,
} from "./aggregateRoutes";
import type { AggregateRoutes } from "@/types";

const initial: AggregateRoutes = {
  slots: [
    {
      routeId: "claude-sonnet-4-6",
      tier: "sonnet",
      providerId: "a",
      upstreamModel: "upstream-a",
      label: "A",
      maxEffort: "high",
      supports1m: true,
    },
    {
      routeId: "claude-sonnet-4-5",
      tier: "sonnet",
      providerId: "b",
      upstreamModel: "upstream-b",
      label: "B",
    },
  ],
  defaultTarget: { kind: "slotId", value: "claude-sonnet-4-6" },
  defaultModel: "claude-sonnet-4-6",
  aliasRules: [{ prefix: "team", slotId: "claude-sonnet-4-6" }],
};

describe("aggregate compatibility contract", () => {
  it("case-variant legacy duplicates require explicit migration and retire the entire ambiguous namespace", () => {
    const routes: AggregateRoutes = {
      ...initial,
      slots: [
        initial.slots[0],
        { ...initial.slots[1], routeId: "CLAUDE-SONNET-4-6" },
      ],
    };
    expect(needsRouteIdMigration(routes)).toBe(true);
    const next = migrateIncompatibleRouteIds(routes);
    expect(
      next.slots.every((s) => s.routeId.toLowerCase() !== "claude-sonnet-4-6"),
    ).toBe(true);
    expect(
      next.retiredRouteIds?.some(
        (id) => id.toLowerCase() === "claude-sonnet-4-6",
      ),
    ).toBe(true);
  });
  it("confirmed migration also normalizes an unsupported cap on an unchanged valid ID", () => {
    const routes: AggregateRoutes = {
      ...initial,
      slots: [
        { ...initial.slots[0], maxEffort: "xhigh" },
        { ...initial.slots[1], routeId: "claude-fable-glm", tier: "fable" },
      ],
    };
    const next = migrateIncompatibleRouteIds(routes);
    expect(next.slots[0].routeId).toBe(initial.slots[0].routeId);
    expect(next.slots[0].maxEffort).toBeUndefined();
  });
  it("explicit duplicate-ID migration removes a strength cap unsupported by the new ID", () => {
    const routes: AggregateRoutes = {
      slots: [
        { ...initial.slots[0], routeId: "claude-sonnet-5", maxEffort: "xhigh" },
        { ...initial.slots[1], routeId: "claude-sonnet-5", maxEffort: "xhigh" },
      ],
      defaultTarget: { kind: "providerId", value: "a" },
    };
    const migrated = migrateIncompatibleRouteIds(routes);
    expect(migrated.slots[0].routeId).toBe("claude-sonnet-4-6");
    expect(migrated.slots[0].maxEffort).toBeUndefined();
    expect(migrated.slots[1].routeId).toBe("claude-sonnet-4-5");
    expect(migrated.slots[1].maxEffort).toBeUndefined();
  });
  it("allocation does not reuse a retired ID with different letter case", () => {
    const routes: AggregateRoutes = {
      slots: [{ ...initial.slots[0], routeId: "" }],
      defaultTarget: { kind: "providerId", value: "a" },
      retiredRouteIds: ["CLAUDE-SONNET-4-6"],
    };
    expect(assignSlotIds(routes).slots[0].routeId).not.toBe(
      "claude-sonnet-4-6",
    );
  });
  it("reordering preserves the old model ID, mapping, default and alias", () => {
    const next = assignSlotIds({
      ...initial,
      slots: [...initial.slots].reverse(),
    });
    expect(
      next.slots.find((s) => s.routeId === initial.slots[0].routeId),
    ).toEqual(initial.slots[0]);
    expect(next.defaultTarget).toEqual(initial.defaultTarget);
    expect(next.defaultModel).toBe(initial.defaultModel);
    expect(next.aliasRules).toEqual(initial.aliasRules);
  });

  it("inserting a new model does not steal an existing ID", () => {
    const next = assignSlotIds({
      ...initial,
      slots: [
        {
          ...initial.slots[0],
          routeId: "",
          providerId: "new",
          upstreamModel: "new-model",
        },
        ...initial.slots,
      ],
    });
    expect(next.slots.slice(1)).toEqual(initial.slots);
    expect(next.slots[0].routeId).not.toBe(initial.slots[0].routeId);
    expect(next.defaultModel).toBe(initial.defaultModel);
  });

  it("deleting then adding and reloading cannot recycle a retired model ID", () => {
    const deleted = assignSlotIds(
      {
        ...initial,
        slots: [initial.slots[1]],
        defaultTarget: { kind: "providerId", value: "b" },
      },
      initial,
    );
    const persisted = JSON.parse(JSON.stringify(deleted)) as AggregateRoutes;
    const added = assignSlotIds(
      {
        ...persisted,
        slots: [
          ...persisted.slots,
          { ...initial.slots[0], routeId: "", upstreamModel: "new-model" },
        ],
      },
      persisted,
    );
    expect(added.slots[0].routeId).toBe(initial.slots[1].routeId);
    expect(added.slots[1].routeId).not.toBe(initial.slots[0].routeId);
    expect(
      (added as AggregateRoutes & { retiredRouteIds?: string[] })
        .retiredRouteIds,
    ).toContain(initial.slots[0].routeId);
    expect(added.aliasRules?.[0].slotId).toBe("");
  });

  it("grouping and flattening retains every model in the same provider/tier", () => {
    const slots = initial.slots.map((s) => ({ ...s, providerId: "a" }));
    expect(flattenProviderGroups(groupSlotsByProvider(slots))).toEqual(slots);
  });
});
