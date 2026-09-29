import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useRef,
} from "react";
import type { UpdateInfo } from "../lib/updater";
import { checkForUpdate } from "../lib/updater";
import { extractErrorMessage } from "../utils/errorUtils";

interface UpdateContextValue {
  // 更新状态
  hasUpdate: boolean;
  updateInfo: UpdateInfo | null;
  isChecking: boolean;
  error: string | null;

  // 提示状态
  isDismissed: boolean;
  dismissUpdate: () => void;

  // 操作方法
  checkUpdate: () => Promise<boolean>;
}

const UpdateContext = createContext<UpdateContextValue | undefined>(undefined);

// dismiss 键由纯版本号升级为「状态指纹」：同一版本下「仅上游修复 PR 合并」与「有新版本」
// 是两种不同状态，需分别记忆（PR 合并后应能再次提醒）。旧值（纯版本号）与新格式不匹配，
// 首次会表现为「未忽略过」——最多多提醒一次，可接受。
const dismissedKeyOf = (info: UpdateInfo): string =>
  `${info.availableVersion}|pr:${info.prMerged ?? false}`;

export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const DISMISSED_VERSION_KEY = "ccswitch:update:dismissedVersion";
  const LEGACY_DISMISSED_KEY = "dismissedUpdateVersion"; // 兼容旧键

  const [hasUpdate, setHasUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isDismissed, setIsDismissed] = useState(false);

  // 当前状态的指纹（版本 + PR 合并状态），空串表示尚无更新信息
  const stateKey = updateInfo ? dismissedKeyOf(updateInfo) : "";

  // 从 localStorage 读取已关闭的状态指纹
  useEffect(() => {
    if (!stateKey) return;

    // 读取新键；若不存在，尝试迁移旧键
    let dismissedVersion = localStorage.getItem(DISMISSED_VERSION_KEY);
    if (!dismissedVersion) {
      const legacy = localStorage.getItem(LEGACY_DISMISSED_KEY);
      if (legacy) {
        localStorage.setItem(DISMISSED_VERSION_KEY, legacy);
        localStorage.removeItem(LEGACY_DISMISSED_KEY);
        dismissedVersion = legacy;
      }
    }

    setIsDismissed(dismissedVersion === stateKey);
  }, [stateKey]);

  const isCheckingRef = useRef(false);

  const checkUpdate = useCallback(async () => {
    if (isCheckingRef.current) return false;
    isCheckingRef.current = true;
    setIsChecking(true);
    setError(null);

    try {
      const result = await checkForUpdate({ timeout: 30000 });

      if (result.status === "available") {
        setHasUpdate(true);
        setUpdateInfo(result.info);

        // 检查是否已经关闭过这个状态的提醒
        let dismissedVersion = localStorage.getItem(DISMISSED_VERSION_KEY);
        if (!dismissedVersion) {
          const legacy = localStorage.getItem(LEGACY_DISMISSED_KEY);
          if (legacy) {
            localStorage.setItem(DISMISSED_VERSION_KEY, legacy);
            localStorage.removeItem(LEGACY_DISMISSED_KEY);
            dismissedVersion = legacy;
          }
        }
        setIsDismissed(dismissedVersion === dismissedKeyOf(result.info));
        return true; // 有更新
      } else {
        setHasUpdate(false);
        setUpdateInfo(null);
        setIsDismissed(false);
        return false; // 已是最新
      }
    } catch (err) {
      console.error("检查更新失败:", err);
      setError(extractErrorMessage(err) || "检查更新失败");
      setHasUpdate(false);
      throw err; // 抛出错误让调用方处理
    } finally {
      setIsChecking(false);
      isCheckingRef.current = false;
    }
  }, []);

  const dismissUpdate = useCallback(() => {
    setIsDismissed(true);
    if (updateInfo) {
      localStorage.setItem(DISMISSED_VERSION_KEY, dismissedKeyOf(updateInfo));
      // 清理旧键
      localStorage.removeItem(LEGACY_DISMISSED_KEY);
    }
  }, [updateInfo]);

  // 应用启动时自动检查更新
  useEffect(() => {
    // 延迟1秒后检查，避免影响启动体验
    const timer = setTimeout(() => {
      checkUpdate().catch(console.error);
    }, 1000);

    return () => clearTimeout(timer);
  }, [checkUpdate]);

  const value: UpdateContextValue = {
    hasUpdate,
    updateInfo,
    isChecking,
    error,
    isDismissed,
    dismissUpdate,
    checkUpdate,
  };

  return (
    <UpdateContext.Provider value={value}>{children}</UpdateContext.Provider>
  );
}

export function useUpdate() {
  const context = useContext(UpdateContext);
  if (!context) {
    throw new Error("useUpdate must be used within UpdateProvider");
  }
  return context;
}
