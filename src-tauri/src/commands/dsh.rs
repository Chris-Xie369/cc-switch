//! DeepSeek Harness 命令层。聚合槽位同步由「应用切换时自动」+「本命令手动」双通道。

use crate::dsh_config;
use crate::store::AppState;
use tauri::State;

/// 把当前 Claude Desktop 聚合槽位立即同步进 `~/.dsh/settings.yaml`
/// （用户从未配置过 cc-switch route 时为 no-op，避免惊扰未启用 DSH 的用户）。
#[tauri::command]
pub fn sync_dsh_aggregate(state: State<'_, AppState>) -> Result<(), String> {
    dsh_config::sync_dsh_aggregate_now(state.db.as_ref(), None).map_err(|e| e.to_string())
}
