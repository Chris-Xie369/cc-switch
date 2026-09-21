use serde::Serialize;

/// 本 fork 跟踪的上游仓库与关键 PR（补丁 A 的来源）。
const UPSTREAM_REPO: &str = "farion1231/cc-switch";
const TARGET_PR: u64 = 5417;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamStatus {
    /// 上游修复 PR（#5417）是否已合并——合并后本地即可丢弃补丁 A。
    pub pr_merged: bool,
    /// 上游最新 release 的 tag（如 "v3.21.0"）；查询失败时为 None。
    pub latest_release: Option<String>,
}

async fn fetch_json(url: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        // GitHub API 强制要求 User-Agent，缺失会返回 403。
        .user_agent("cc-switch-local-upstream-check")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// 查询上游状态：PR #5417 是否合并 + 最新 release。
/// 两个请求独立容错；都失败才报错（前端据此显示"检查失败"）。
#[tauri::command]
pub async fn check_upstream_status() -> Result<UpstreamStatus, String> {
    let pr = fetch_json(&format!(
        "https://api.github.com/repos/{UPSTREAM_REPO}/pulls/{TARGET_PR}"
    ))
    .await;
    let release = fetch_json(&format!(
        "https://api.github.com/repos/{UPSTREAM_REPO}/releases/latest"
    ))
    .await;

    let pr_merged = pr
        .as_ref()
        .ok()
        .and_then(|v| v.get("merged"))
        .and_then(|m| m.as_bool())
        .unwrap_or(false);
    let latest_release = release
        .as_ref()
        .ok()
        .and_then(|v| v.get("tag_name"))
        .and_then(|t| t.as_str())
        .map(str::to_string);

    if pr.is_err() && release.is_err() {
        return Err("无法获取上游状态（网络不可达或 API 限流）".to_string());
    }

    Ok(UpstreamStatus {
        pr_merged,
        latest_release,
    })
}
