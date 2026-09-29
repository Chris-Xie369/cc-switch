import { describe, expect, it } from "vitest";
import { effortCapability, effortLevelsFor } from "./claudeDesktopCapability";

describe("effortCapability（三态，镜像 Desktop asar czt/lzt/szt）", () => {
  it("完整阶梯：精确表 ID 与 fable/mythos 族", () => {
    expect(effortCapability("claude-sonnet-4-6")).toBe("ladder");
    expect(effortCapability("claude-sonnet-5")).toBe("ladder");
    expect(effortCapability("claude-opus-4-8")).toBe("ladder");
    expect(effortCapability("claude-fable-5")).toBe("ladder");
    expect(effortCapability("mythos-1")).toBe("ladder");
  });

  it("仅扩展思考开关：sonnet-4-5 与 haiku-4-5", () => {
    expect(effortCapability("claude-sonnet-4-5")).toBe("extended");
    expect(effortCapability("claude-haiku-4-5")).toBe("extended");
  });

  it("无：溢出 ID 与垃圾输入", () => {
    expect(effortCapability("claude-sonnet-3")).toBe("none");
    expect(effortCapability("claude-haiku-2")).toBe("none");
    expect(effortCapability("claude-haiku-3")).toBe("none");
    expect(effortCapability("claude-opus-4")).toBe("none");
    expect(effortCapability("garbage")).toBe("none");
  });

  it("剥 [1m] 后缀（大小写不敏感）与小写化后再判定", () => {
    expect(effortCapability("claude-sonnet-5[1m]")).toBe("ladder");
    expect(effortCapability("CLAUDE-FABLE-1[1M]")).toBe("ladder");
    expect(effortCapability("claude-sonnet-4-5[1m] ")).toBe("extended");
  });

  it("后缀剥离与族判定都大小写不敏感、族正则带尾锚", () => {
    // 大写后缀：replace 无 /i 时 "CLAUDE-SONNET-5[1M]" 剥不掉 [+1M]，
    // 小写化后落在表外 → none（本断言杀该变异体）
    expect(effortCapability("CLAUDE-SONNET-5[1M]")).toBe("ladder");
    // claude-fablex 不是 fable 族：族正则去掉 (?:-|$) 锚会误判为 ladder
    expect(effortCapability("claude-fablex")).toBe("none");
    // Object.prototype 上的键名不是模型 ID：用 `in` 查表会命中原型链而误报 ladder
    expect(effortCapability("constructor")).toBe("none");
    expect(effortLevelsFor("constructor")).toEqual([]);
  });
});

describe("effortLevelsFor（阶梯明细，供 maxEffort 下拉）", () => {
  it("4-6 与 opus-4-6 无 xhigh；sonnet-5/opus-4-7/4-8 有", () => {
    expect(effortLevelsFor("claude-sonnet-4-6")).toEqual(["low","medium","high","max"]);
    expect(effortLevelsFor("claude-opus-4-6")).toEqual(["low","medium","high","max"]);
    expect(effortLevelsFor("claude-opus-4-8")).toEqual(["low","medium","high","xhigh","max"]);
    expect(effortLevelsFor("claude-sonnet-5")).toContain("xhigh");
    expect(effortLevelsFor("claude-fable-1")).toEqual(["low","medium","high","xhigh","max"]);
  });

  it("非 ladder 返回空数组", () => {
    expect(effortLevelsFor("claude-haiku-4-5")).toEqual([]);
    expect(effortLevelsFor("claude-sonnet-3")).toEqual([]);
  });
});
