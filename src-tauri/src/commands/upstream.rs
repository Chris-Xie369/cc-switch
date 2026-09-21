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

/// 上游 tag（如 "v3.21.0"）是否比本地版本（如 "3.20.3-local"）更新。
/// 忽略 tag 的 "v" 前缀，以及两侧的预发布/构建元数据（"-local"、"-beta.1"、"+build"）。
pub fn is_upstream_newer(latest_tag: &str, local_version: &str) -> bool {
    fn parse(s: &str) -> Option<(u64, u64, u64)> {
        let core = s.trim_start_matches('v').split(['-', '+']).next()?;
        let mut it = core.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next()?.parse().ok()?;
        let patch = it.next().unwrap_or("0").parse().ok()?;
        Some((major, minor, patch))
    }
    match (parse(latest_tag), parse(local_version)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_upstream_newer_detects_higher_version() {
        assert!(is_upstream_newer("v3.21.0", "3.20.3-local"));
        assert!(is_upstream_newer("3.20.4", "3.20.3-local"));
        assert!(is_upstream_newer("v4.0.0", "3.20.3-local"));
    }

    #[test]
    fn is_upstream_newer_is_false_for_same_or_lower() {
        assert!(!is_upstream_newer("v3.20.3", "3.20.3-local"));
        assert!(!is_upstream_newer("v3.20.2", "3.20.3-local"));
    }

    #[test]
    fn is_upstream_newer_is_false_for_unparsable() {
        assert!(!is_upstream_newer("", "3.20.3-local"));
        assert!(!is_upstream_newer("latest", "3.20.3-local"));
    }
}
