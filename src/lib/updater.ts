import { getVersion } from "@tauri-apps/api/app";

export type UpdateChannel = "stable" | "beta";

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes?: string;
  pubDate?: string;
  /** 上游修复 PR（#5417）是否已合并——合并后本地可丢弃补丁 A */
  prMerged?: boolean;
}

export interface CheckOptions {
  timeout?: number;
  channel?: UpdateChannel;
}

export interface UpstreamStatus {
  prMerged: boolean;
  latestRelease: string | null;
}

export async function getCurrentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

/** 上游 tag 是否比本地版本更新（忽略 v 前缀与 -local 等预发布后缀）。 */
function isUpstreamNewer(latestTag: string, localVersion: string): boolean {
  const parse = (s: string): [number, number, number] | null => {
    const core = s.replace(/^v/, "").split(/[-+]/)[0];
    const parts = core.split(".");
    const nums = [parts[0], parts[1], parts[2] ?? "0"].map((p) =>
      Number.parseInt(p ?? "", 10),
    );
    if (nums.some((n) => Number.isNaN(n))) return null;
    return nums as [number, number, number];
  };
  const a = parse(latestTag);
  const b = parse(localVersion);
  if (!a || !b) return false;
  for (let i = 0; i < 3; i += 1) {
    if (a[i] !== b[i]) return a[i] > b[i];
  }
  return false;
}

// 本机为本地构建（3.20.3-local）且 updater 插件已禁用（补丁 C），原「检查更新」必然失败。
// 改为查询上游状态：修复 PR（#5417）是否合并 + 最新 release，据此提示同步时机。
export async function checkForUpdate(
  _opts: CheckOptions = {},
): Promise<
  { status: "up-to-date" } | { status: "available"; info: UpdateInfo }
> {
  const { invoke } = await import("@tauri-apps/api/core");
  const status = await invoke<UpstreamStatus>("check_upstream_status");

  const currentVersion = await getCurrentVersion();
  const latest = status.latestRelease ?? "";
  const newer = latest ? isUpstreamNewer(latest, currentVersion) : false;

  // 只有「上游有更新版本」或「修复 PR 已合并」才提示；两者都无变化时静默。
  if (!newer && !status.prMerged) {
    return { status: "up-to-date" };
  }

  const info: UpdateInfo = {
    currentVersion,
    availableVersion: newer ? latest : currentVersion,
    prMerged: status.prMerged,
  };

  return { status: "available", info };
}
