import { describe, expect, it } from "vitest";
import { assignSlotIds, canSaveAggregateRoutes, slotId } from "./aggregateRoutes";

describe("slotId", () => {
  it("只由档位与序号构成", () => {
    expect(slotId("sonnet", 1)).toBe("claude-sonnet-1");
    expect(slotId("fable", 2)).toBe("claude-fable-2");
  });

  it("ID 里绝不出现供应商名（厂商词会让 Claude Desktop 整组丢弃模型列表）", () => {
    // 实测（Claude Desktop 2.2553.1.0，看门狗日志已确证）：模型列表里凡是名字含
    // deepseek/glm/kimi/gpt/qwen/gemini… 这类**厂商词**的条目，都会被判为
    // "is not an Anthropic model" 从列表移除。旧方案 claude-{tier}-{供应商名 slug}
    // 恰好撞上这条黑名单（claude-fable-deepseek、claude-fable-zhipu-glm），
    // 四个槽位被删光、选择器变空。ID 必须与供应商名无关，可读性交给「显示名」。
    for (const tier of ["sonnet", "opus", "haiku", "fable"] as const) {
      const id = slotId(tier, 1);
      expect(id).toBe(`claude-${tier}-1`);
      expect(id).not.toMatch(/deepseek|glm|kimi|gpt|gemini|qwen/i);
    }
  });

  it("产物满足后端 is_claude_safe_model_id 形状", () => {
    // claude-{sonnet|opus|haiku|fable}-{非空}
    expect(slotId("haiku", 3)).toMatch(/^claude-haiku-.+$/);
  });
});

describe("canSaveAggregateRoutes", () => {
  const slot = {
    routeId: "claude-sonnet-glm",
    tier: "sonnet" as const,
    providerId: "p-glm",
    upstreamModel: "glm-5.3",
  };

  it("需要至少一个槽位与一个非空默认目标值", () => {
    expect(
      canSaveAggregateRoutes({
        slots: [slot],
        defaultTarget: { kind: "providerId", value: "p-glm" },
      }),
    ).toBe(true);
  });

  it("默认目标值为空白（未选择）时不可保存", () => {
    expect(
      canSaveAggregateRoutes({
        slots: [slot],
        defaultTarget: { kind: "providerId", value: "" },
      }),
    ).toBe(false);
    expect(
      canSaveAggregateRoutes({
        slots: [slot],
        defaultTarget: { kind: "slotId", value: "   " },
      }),
    ).toBe(false);
  });

  it("无槽位或路由表为空时不可保存", () => {
    expect(
      canSaveAggregateRoutes({
        slots: [],
        defaultTarget: { kind: "providerId", value: "p-glm" },
      }),
    ).toBe(false);
    expect(canSaveAggregateRoutes(undefined)).toBe(false);
    expect(canSaveAggregateRoutes(null)).toBe(false);
  });
});

describe("assignSlotIds", () => {
  const base = {
    defaultTarget: { kind: "providerId" as const, value: "p-glm" },
  };

  it("按档位分别编号：同档第 1、2 个得到 -1、-2", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "",
          tier: "sonnet",
          providerId: "p-ds",
          upstreamModel: "flash",
        },
        {
          routeId: "",
          tier: "sonnet",
          providerId: "p-glm",
          upstreamModel: "pro",
        },
      ],
    });
    expect(next.slots.map((s) => s.routeId)).toEqual([
      "claude-sonnet-1",
      "claude-sonnet-2",
    ]);
  });

  it("不同档位各自从 1 开始", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        { routeId: "", tier: "sonnet", providerId: "p-ds", upstreamModel: "a" },
        { routeId: "", tier: "opus", providerId: "p-ds", upstreamModel: "b" },
        { routeId: "", tier: "sonnet", providerId: "p-glm", upstreamModel: "c" },
      ],
    });
    expect(next.slots.map((s) => s.routeId)).toEqual([
      "claude-sonnet-1",
      "claude-opus-1",
      "claude-sonnet-2",
    ]);
  });

  it("档位变化会改变 ID", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "claude-sonnet-1",
          tier: "opus",
          providerId: "p-ds",
          upstreamModel: "flash",
        },
      ],
    });
    expect(next.slots[0].routeId).toBe("claude-opus-1");
  });

  it("重算所有槽位，旧的 routeId 不会残留", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "claude-sonnet-stale",
          tier: "opus",
          providerId: "p-ds",
          upstreamModel: "flash",
        },
        {
          routeId: "claude-sonnet-stale",
          tier: "sonnet",
          providerId: "p-ds",
          upstreamModel: "pro",
        },
      ],
    });
    expect(next.slots.map((s) => s.routeId)).toEqual([
      "claude-opus-1",
      "claude-sonnet-1",
    ]);
  });

  it("旧方案（含供应商名）的存量 ID 会被迁移掉", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "claude-fable-zhipu-glm",
          tier: "fable",
          providerId: "p-glm",
          upstreamModel: "glm-5.3",
        },
      ],
    });
    expect(next.slots[0].routeId).toBe("claude-fable-1");
  });

  it("默认目标引用槽位 ID 时，按位置跟随重编号", () => {
    const next = assignSlotIds({
      defaultTarget: { kind: "slotId", value: "claude-fable-zhipu-glm" },
      slots: [
        {
          routeId: "claude-opus-deepseek",
          tier: "opus",
          providerId: "p-ds",
          upstreamModel: "a",
        },
        {
          routeId: "claude-fable-zhipu-glm",
          tier: "fable",
          providerId: "p-glm",
          upstreamModel: "b",
        },
      ],
    });
    expect(next.defaultTarget).toEqual({
      kind: "slotId",
      value: "claude-fable-1",
    });
  });

  it("目标槽位已被删除时保持原值，绝不静默改指", () => {
    const next = assignSlotIds({
      defaultTarget: { kind: "slotId", value: "claude-fable-gone" },
      slots: [
        {
          routeId: "claude-sonnet-1",
          tier: "sonnet",
          providerId: "p-ds",
          upstreamModel: "a",
        },
      ],
    });
    expect(next.defaultTarget).toEqual({
      kind: "slotId",
      value: "claude-fable-gone",
    });
  });
});
