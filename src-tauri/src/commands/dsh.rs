//! DeepSeek Harness 命令层。聚合槽位同步由「应用切换时自动」+「本命令手动」双通道。

use crate::dsh_config;
use crate::store::AppState;
use tauri::State;

/// Refresh the existing CC Switch Aggregate route from the current Desktop Stack.
/// Stack off, or a user who never created the route, leaves the files alone.
#[tauri::command]
pub fn sync_dsh_aggregate(state: State<'_, AppState>) -> Result<(), String> {
    if !crate::mode::stack::stack_mode_now(&crate::app_config::AppType::ClaudeDesktop) {
        return Ok(());
    }
    let models =
        crate::mode::stack::desktop_published_now(state.db.as_ref()).map_err(|e| e.to_string())?;
    dsh_config::sync_dsh_aggregate_now(state.db.as_ref(), &models).map_err(|e| e.to_string())
}
