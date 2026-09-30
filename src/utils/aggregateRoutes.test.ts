import { describe, expect, it } from "vitest";
import type { AggregateAliasRule } from "@/types";
import {
  assignSlotIds,
  canSaveAggregateRoutes,
  flattenProviderGroups,
  groupSlotsByProvider,
  slotId,
  slotLabel,
  TIER_ROW_ORDER,
} from "./aggregateRoutes";

const TIERS = ["sonnet", "opus", "haiku", "fable"] as const;

describe("slotId", () => {
  it("优先取 Claude Desktop 认得的真实 ID（推理强度控件只认这些）", () => {
    expect(slotId("opus", 1)).toBe("claude-opus-4-8");
    expect(slotId("opus", 2)).toBe("claude-opus-4-7");
    expect(slotId("sonnet", 1)).toBe("claude-sonnet-4-6");
    expect(slotId("sonnet", 3)).toBe("claude-sonnet-5");
    expect(slotId("haiku", 1)).toBe("claude-haiku-4-5");
  });

  it("fable 族按序号生成（该族走正则，任意序号都有强度阶梯）", () => {
    expect(slotId("fable", 1)).toBe("claude-fable-1");
    expect(slotId("fable", 7)).toBe("claude-fable-7");
  });

  it("溢出让开池内已占用的名字（claude-sonnet-5 在池内，序号 5 顺移）", () => {
    expect(slotId("sonnet", 5)).toBe("claude-sonnet-6");
    expect(slotId("sonnet", 6)).toBe("claude-sonnet-6"); // 单次调用无状态，见下条批量保证
  });

  it("池子用尽后退回 claude-{档位}-{序号}", () => {
    expect(slotId("sonnet", 2)).toBe("claude-sonnet-4-5");
    expect(slotId("sonnet", 4)).toBe("claude-sonnet-4");
    expect(slotId("haiku", 2)).toBe("claude-haiku-2");
    expect(slotId("opus", 4)).toBe("claude-opus-4");
  });

  it("同档位的 ID 互不相同（assignSlotIds 批量路径，池内 + 溢出混合）", () => {
    // 池里若混进 claude-{档位}-{数字} 形状的 ID，就会和溢出生成值撞名，
    // 两条槽位同 ID 会被后端去重吃掉一条（claude-sonnet-5 踩过）。
    // 批量分配时溢出还会让开本次已分配的 ID（如序号 5 让出 sonnet-5 后取 6，
    // 序号 6 不能再取 6）。唯一性由 assignSlotIds 的有状态分配保证。
    for (const tier of TIERS) {
      const routes = {
        slots: Array.from({ length: 20 }, () => ({
          routeId: "",
          tier,
          providerId: "p",
          upstreamModel: "m",
          supports1m: false,
        })),
        defaultTarget: { kind: "providerId" as const, value: "p" },
      };
      const ids = assignSlotIds(routes).slots.map((s) => s.routeId);
      expect(new Set(ids).size).toBe(ids.length);
    }
  });

  it("ID 里绝不出现供应商名（厂商词会让 Claude Desktop 整组丢弃模型列表）", () => {
    // 实测（Claude Desktop 2.2553.1.0，main.log 已确证）：模型列表里凡是名字含
    // deepseek/glm/kimi/gpt/qwen/gemini… 这类**厂商词**的条目，都会被判为
    // "is not an Anthropic model" 从列表移除。旧方案 claude-{tier}-{供应商名 slug}
    // 恰好撞上这条黑名单（claude-fable-deepseek、claude-fable-zhipu-glm），
    // 四个槽位被删光、选择器变空。ID 必须与供应商名无关，可读性交给「显示名」。
    for (const tier of TIERS) {
      for (let ordinal = 1; ordinal <= 20; ordinal += 1) {
        expect(slotId(tier, ordinal)).not.toMatch(
          /deepseek|glm|kimi|gpt|gemini|qwen/i,
        );
      }
    }
  });

  it("产物满足后端 is_claude_safe_model_id 形状", () => {
    // claude-{sonnet|opus|haiku|fable}-{非空}
    for (const tier of TIERS) {
      for (let ordinal = 1; ordinal <= 20; ordinal += 1) {
        expect(slotId(tier, ordinal)).toMatch(
          /^claude-(sonnet|opus|haiku|fable)-.+$/,
        );
      }
    }
  });
});

describe("slotLabel", () => {
  it("显式显示名优先（去掉首尾空白）", () => {
    expect(
      slotLabel(
        { upstreamModel: "glm-5.3", label: "  智谱 GLM  " },
        "智谱 GLM",
      ),
    ).toBe("智谱 GLM");
  });

  it("未填写时回落为「供应商 · 上游模型」", () => {
    expect(slotLabel({ upstreamModel: "glm-5.3" }, "智谱 GLM")).toBe(
      "智谱 GLM · glm-5.3",
    );
    expect(
      slotLabel({ upstreamModel: "glm-5.3", label: "   " }, "智谱 GLM"),
    ).toBe("智谱 GLM · glm-5.3");
  });

  it("缺任一侧时只显示存在的一侧", () => {
    expect(slotLabel({ upstreamModel: "glm-5.3" })).toBe("glm-5.3");
    expect(slotLabel({ upstreamModel: "  " }, "智谱 GLM")).toBe("智谱 GLM");
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

  it("按档位分别编号：同档第 1、2 个依次取该档 ID 池", () => {
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
      "claude-sonnet-4-6",
      "claude-sonnet-4-5",
    ]);
  });

  it("不同档位各自从池首开始", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        { routeId: "", tier: "sonnet", providerId: "p-ds", upstreamModel: "a" },
        { routeId: "", tier: "opus", providerId: "p-ds", upstreamModel: "b" },
        {
          routeId: "",
          tier: "sonnet",
          providerId: "p-glm",
          upstreamModel: "c",
        },
      ],
    });
    expect(next.slots.map((s) => s.routeId)).toEqual([
      "claude-sonnet-4-6",
      "claude-opus-4-8",
      "claude-sonnet-4-5",
    ]);
  });

  it("档位变化会改变 ID", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "claude-sonnet-4-6",
          tier: "opus",
          providerId: "p-ds",
          upstreamModel: "flash",
        },
      ],
    });
    expect(next.slots[0].routeId).toBe("claude-opus-4-8");
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
      "claude-opus-4-8",
      "claude-sonnet-4-6",
    ]);
  });

  it("上一版序号方案（claude-opus-1）的存量 ID 会被迁移成池内 ID", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "claude-opus-1",
          tier: "opus",
          providerId: "p-glm",
          upstreamModel: "glm-5.3-flash",
        },
        {
          routeId: "claude-opus-2",
          tier: "opus",
          providerId: "p-ds",
          upstreamModel: "deepseek-flash",
        },
      ],
    });
    expect(next.slots.map((s) => s.routeId)).toEqual([
      "claude-opus-4-8",
      "claude-opus-4-7",
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

  it("assignSlotIds 保留 maxEffort 字段", () => {
    const routes = {
      slots: [
        {
          routeId: "claude-sonnet-5",
          tier: "sonnet" as const,
          providerId: "p",
          upstreamModel: "m",
          supports1m: false,
          maxEffort: "xhigh" as const,
        },
      ],
      defaultTarget: { kind: "providerId" as const, value: "p" },
    };
    expect(assignSlotIds(routes).slots[0].maxEffort).toBe("xhigh");
  });
});

describe("groupSlotsByProvider / flattenProviderGroups", () => {
  const slot = (
    routeId: string,
    tier: "fable" | "opus" | "sonnet" | "haiku",
    providerId: string,
    upstreamModel: string,
  ) => ({ routeId, tier, providerId, upstreamModel });

  it("卡序 = 供应商在扁平列表里的首次出现顺序（槽位交错也不乱）", () => {
    const cards = groupSlotsByProvider([
      slot("a", "fable", "p-glm", "glm-5.3"),
      slot("b", "opus", "p-ds", "deepseek-flash"),
      slot("c", "opus", "p-glm", "glm-5.3-flash"),
    ]);
    expect(cards.map((card) => card.providerId)).toEqual(["p-glm", "p-ds"]);
    expect(Object.keys(cards[0].rows)).toEqual(["fable", "opus"]);
    expect(cards[1].rows.opus?.upstreamModel).toBe("deepseek-flash");
  });

  it("同供应商同档位的存量重复取首个（新 UI 固定档位行造不出重复）", () => {
    const cards = groupSlotsByProvider([
      slot("keep", "opus", "p-glm", "glm-5.3"),
      slot("drop", "opus", "p-glm", "glm-5.3-flash"),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].rows.opus?.routeId).toBe("keep");
  });

  it("空列表得空卡片数组", () => {
    expect(groupSlotsByProvider([])).toEqual([]);
  });

  it("展平按卡序 × 档位固定顺序重建（fable→opus→sonnet→haiku）", () => {
    const flat = flattenProviderGroups([
      { providerId: "p-ds", rows: { opus: slot("x", "opus", "p-ds", "d1") } },
      {
        providerId: "p-glm",
        rows: {
          haiku: slot("h", "haiku", "p-glm", "g3"),
          fable: slot("f", "fable", "p-glm", "g1"),
        },
      },
    ]);
    expect(flat.map((s) => [s.providerId, s.tier])).toEqual([
      ["p-ds", "opus"],
      ["p-glm", "fable"],
      ["p-glm", "haiku"],
    ]);
  });

  it("未映射的档位不产生槽位（映射几个就有几个）", () => {
    const flat = flattenProviderGroups([
      {
        providerId: "p-glm",
        rows: { fable: slot("f", "fable", "p-glm", "g1") },
      },
    ]);
    expect(flat).toHaveLength(1);
  });

  it("档位行序常量与卡片渲染一致", () => {
    expect(TIER_ROW_ORDER).toEqual(["fable", "opus", "sonnet", "haiku"]);
  });
});

describe("assignSlotIds · defaultModel 跟随", () => {
  const base = {
    defaultTarget: { kind: "providerId" as const, value: "p-glm" },
  };

  it("引用的槽位重编号后按位置跟随", () => {
    const next = assignSlotIds({
      ...base,
      defaultModel: "claude-opus-deepseek",
      slots: [
        {
          routeId: "claude-fable-zhipu-glm",
          tier: "fable",
          providerId: "p-glm",
          upstreamModel: "glm-5.3",
        },
        {
          routeId: "claude-opus-deepseek",
          tier: "opus",
          providerId: "p-ds",
          upstreamModel: "deepseek-flash",
        },
      ],
    });
    expect(next.defaultModel).toBe("claude-opus-4-8");
  });

  it("引用的槽位被删时置空（回落到排序首位）", () => {
    const next = assignSlotIds({
      ...base,
      defaultModel: "claude-fable-gone",
      slots: [
        {
          routeId: "claude-sonnet-1",
          tier: "sonnet",
          providerId: "p-ds",
          upstreamModel: "a",
        },
      ],
    });
    expect(next.defaultModel).toBeUndefined();
  });

  it("未设置时保持未设置", () => {
    const next = assignSlotIds({
      ...base,
      slots: [
        {
          routeId: "claude-sonnet-1",
          tier: "sonnet",
          providerId: "p-ds",
          upstreamModel: "a",
        },
      ],
    });
    expect(next.defaultModel).toBeUndefined();
  });
});

describe("assignSlotIds aliasRules 跟随", () => {
  const base = {
    slots: [
      {
        routeId: "claude-sonnet-5",
        tier: "sonnet" as const,
        providerId: "p1",
        upstreamModel: "m1",
        supports1m: false,
      },
      {
        routeId: "claude-haiku-3",
        tier: "haiku" as const,
        providerId: "p1",
        upstreamModel: "m2",
        supports1m: false,
      },
    ],
    defaultTarget: { kind: "providerId" as const, value: "p1" },
  };

  it("引用的槽被重编号时规则跟随到新 ID", () => {
    const aliasRules: AggregateAliasRule[] = [
      { prefix: "claude-haiku", slotId: "claude-haiku-3" },
    ];
    const routes = { ...base, aliasRules };
    // 在最前插入一个 haiku 槽：新槽拿池首 claude-haiku-4-5，原 haiku-3 槽序号 2 溢出为 claude-haiku-2
    const inserted = {
      ...routes,
      slots: [
        {
          routeId: "",
          tier: "haiku" as const,
          providerId: "p1",
          upstreamModel: "m0",
          supports1m: false,
        },
        ...routes.slots,
      ],
    };
    const out = assignSlotIds(inserted);
    expect(out.slots[2].routeId).toBe("claude-haiku-2");
    expect(out.aliasRules?.[0]).toEqual({
      prefix: "claude-haiku",
      slotId: "claude-haiku-2",
    });
  });

  it("引用的槽已删时 slotId 清空、行保留", () => {
    const routes = {
      ...base,
      slots: base.slots.slice(0, 1), // 删掉 haiku 槽
      aliasRules: [{ prefix: "claude-haiku", slotId: "claude-haiku-3" }],
    };
    const out = assignSlotIds(routes);
    expect(out.aliasRules).toEqual([{ prefix: "claude-haiku", slotId: "" }]);
  });

  it("无 aliasRules 的旧数据往返后仍为 undefined", () => {
    expect(assignSlotIds(base).aliasRules).toBeUndefined();
  });
});
