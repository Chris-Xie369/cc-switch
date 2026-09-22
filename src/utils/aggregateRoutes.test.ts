import { describe, expect, it } from "vitest";
import {
  assignSlotIds,
  canSaveAggregateRoutes,
  generateSlotId,
  slugify,
} from "./aggregateRoutes";

describe("slugify", () => {
  it("规范 ASCII 名称", () => {
    expect(slugify("DeepSeek-OTN", "abc12345")).toBe("deepseek-otn");
    expect(slugify("OpenCode Go", "abc12345")).toBe("opencode-go");
  });

  it("混合名取 ASCII 部分", () => {
    expect(slugify("智谱 GLM", "97a1d0df-b9a9")).toBe("glm");
  });

  it("纯非 ASCII 名回落为 provider id 前缀", () => {
    expect(slugify("月之暗面", "97a1d0df-b9a9")).toBe("97a1d0df");
  });

  it("含非 ASCII 但有 ASCII 时取 ASCII，不回落", () => {
    // 回落只在 ASCII 派生结果为空时触发，而非「名称含非 ASCII」就触发
    expect(slugify("GLM 智谱", "97a1d0df-b9a9")).toBe("glm");
  });

  it("截断并清理首尾分隔符", () => {
    expect(slugify("  ---A--B---  ", "x")).toBe("a-b");
    expect(slugify("abcdefghijklmnopqrstuvwxyz", "x")).toHaveLength(20);
  });

  it("截断正好落在分隔符上时，去掉尾部 '-'", () => {
    // "abcdefghijklmnopqrs tuv" -> 前 19 字母 + 空格 + 3 字母；
    // 先截断到 20 字符会得到 "...s-"（第 20 位是分隔符），必须再去尾 '-'
    const out = slugify("abcdefghijklmnopqrs tuv", "x");
    expect(out).toBe("abcdefghijklmnopqrs");
    expect(out).toHaveLength(19);
  });
});

describe("generateSlotId", () => {
  it("生成 claude-{tier}-{slug}", () => {
    expect(generateSlotId("sonnet", "DeepSeek-OTN", "pid", [])).toBe(
      "claude-sonnet-deepseek-otn",
    );
  });

  it("冲突时追加编号", () => {
    expect(generateSlotId("sonnet", "GLM", "pid", ["claude-sonnet-glm"])).toBe(
      "claude-sonnet-glm-2",
    );
  });

  it("跳过已占用的编号，而非只测 base 本身", () => {
    // taken 同时含 base 与 base-2，下一次必须是 base-3
    expect(
      generateSlotId("sonnet", "GLM", "pid", [
        "claude-sonnet-glm",
        "claude-sonnet-glm-2",
      ]),
    ).toBe("claude-sonnet-glm-3");
  });

  it("产物满足后端 is_claude_safe_model_id 形状", () => {
    // claude-{sonnet|opus|haiku|fable}-{非空}
    expect(generateSlotId("haiku", "月之暗面", "97a1d0df-b9a9", [])).toBe(
      "claude-haiku-97a1d0df",
    );
    expect(generateSlotId("fable", "GLM", "pid", [])).toBe("claude-fable-glm");
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
  const providers = [
    { id: "p-glm", name: "智谱 GLM" },
    { id: "p-ds", name: "DeepSeek-OTN" },
  ];
  const base = {
    defaultTarget: { kind: "providerId" as const, value: "p-glm" },
  };

  it("按目标供应商名称生成 ID，并处理同名冲突", () => {
    const next = assignSlotIds(
      {
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
            providerId: "p-ds",
            upstreamModel: "pro",
          },
        ],
      },
      providers,
    );
    // "DeepSeek-OTN" → slug "deepseek-otn"；第二条同供应商 → 追加编号
    expect(next.slots[0].routeId).toBe("claude-sonnet-deepseek-otn");
    expect(next.slots[1].routeId).toBe("claude-sonnet-deepseek-otn-2");
  });

  it("档位变化会改变 ID", () => {
    const next = assignSlotIds(
      {
        ...base,
        slots: [
          {
            routeId: "",
            tier: "opus",
            providerId: "p-ds",
            upstreamModel: "flash",
          },
        ],
      },
      providers,
    );
    expect(next.slots[0].routeId).toBe("claude-opus-deepseek-otn");
  });

  it("重算所有槽位，旧的 routeId 不会残留", () => {
    // 第 0 槽改为 opus，第 1 槽的陈旧 routeId 也必须按新档位重算
    const next = assignSlotIds(
      {
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
      },
      providers,
    );
    expect(next.slots.map((s) => s.routeId)).toEqual([
      "claude-opus-deepseek-otn",
      "claude-sonnet-deepseek-otn",
    ]);
  });
});
