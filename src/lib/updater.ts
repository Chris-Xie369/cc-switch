import { getVersion } from "@tauri-apps/api/app";
import { isUpdateAvailable } from "@/lib/version";

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
  // 本地版本形如 "3.20.3-local"：semver 中预发布版低于同号正式版，直接比较会把
  // 「上游仍停在 fork 基线」误判为有新版（永久误报），故先剥掉预发布后缀再比。
  const baseVersion = currentVersion.replace(/-.*$/, "");
  // 上游 tag 形如 "v3.21.0"：version.ts 的解析器不认 "v" 前缀，同样先剥掉。
  const latest = (status.latestRelease ?? "").replace(/^v/, "");
  const newer = latest ? isUpdateAvailable(baseVersion, latest) : false;

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
