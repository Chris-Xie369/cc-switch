import { useUpdate } from "@/contexts/UpdateContext";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ArrowUpCircle } from "lucide-react";

interface UpdateBadgeProps {
  className?: string;
  onClick?: () => void;
}

export function UpdateBadge({ className = "", onClick }: UpdateBadgeProps) {
  const { hasUpdate, updateInfo } = useUpdate();
  const { t } = useTranslation();
  const isActive = hasUpdate && updateInfo;
  const availableVersion = updateInfo?.availableVersion ?? "";
  // 「仅 PR 合并、上游无新版本」时 newer 为 false，此时应提示上游修复已合并，
  // 而不是「检测到新版本：<当前版本>」。
  const isNewerVersion = updateInfo?.newer ?? false;
  const title = !isActive
    ? t("settings.checkForUpdates")
    : isNewerVersion
      ? t("settings.updateAvailable", { version: availableVersion })
      : t("settings.upstreamPrMerged");

  if (!isActive) {
    return null;
  }

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      title={title}
      aria-label={title}
      onClick={onClick}
      className={`
        relative h-8 w-8 rounded-full
        ${isActive ? "text-green-600 dark:text-green-400 hover:bg-green-50 dark:hover:bg-green-500/10" : "text-muted-foreground hover:bg-muted/60"}
        ${className}
      `}
    >
      <ArrowUpCircle className="h-5 w-5" />
    </Button>
  );
}
