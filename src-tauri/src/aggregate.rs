//! 聚合供应商：无端点无凭据的虚拟供应商，按模型把请求分流到其他供应商。

use serde::{Deserialize, Serialize};

/// 档位：决定 Claude Desktop 选择器里那句描述文字来自目录中哪个角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregateTier {
    Sonnet,
    Opus,
    Haiku,
    Fable,
}

impl AggregateTier {
    pub fn as_str(self) -> &'static str {
        match self {
            AggregateTier::Sonnet => "sonnet",
            AggregateTier::Opus => "opus",
            AggregateTier::Haiku => "haiku",
            AggregateTier::Fable => "fable",
        }
    }
}

/// 一个槽位 = 一个可被 Claude 选择的模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateRouteSlot {
    /// 生成的槽位 ID（**持久化**，是稳定的路由键）。
    /// 保存时由 `generate_slot_id` 依据目标供应商的名称生成并写入；
    /// 运行时直接用它查表，不再重新生成——这样重命名供应商不会改变已生效的路由。
    pub route_id: String,
    pub tier: AggregateTier,
    /// 目标供应商 id（被引用者受删除保护）
    pub provider_id: String,
    /// 该供应商的上游模型名
    pub upstream_model: String,
    /// 选择器显示名，如 "智谱 GLM-5.3"；缺省用上游模型名
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 勾选后该模型在选择器里会多出一行 "1M context window"
    #[serde(default)]
    pub supports_1m: bool,
}

/// 未命中路由时的兜底目标。按槽位 ID 或供应商 id 引用（不用下标——下标会随增删重排失效）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum DefaultTarget {
    SlotId(String),
    ProviderId(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateRoutes {
    pub slots: Vec<AggregateRouteSlot>,
    pub default_target: DefaultTarget,
}

/// 供应商名称 → slug。规则：**只保留 ASCII 字母数字**（其余字符视作分隔符并合并）→
/// 转小写 → 去首尾 '-' → 截断 20 字符。**仅当结果为空**（名称为纯非 ASCII，如 "月之暗面"）
/// 才回落为 provider_id 前 8 位。
/// 注意："智谱 GLM" 的 ASCII 部分是 GLM → slug 为 "glm"，**不会**回落。
pub fn slugify(provider_name: &str, provider_id: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in provider_name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let truncated = trimmed.chars().take(20).collect::<String>();
    let truncated = truncated.trim_matches('-').to_string();
    if truncated.is_empty() {
        provider_id.chars().take(8).collect()
    } else {
        truncated
    }
}

/// 生成槽位 ID：`claude-{tier}-{slug}`；与 taken 冲突时追加 `-2`、`-3`…
pub fn generate_slot_id(
    tier: AggregateTier,
    provider_name: &str,
    provider_id: &str,
    taken: &[String],
) -> String {
    let base = format!("claude-{}-{}", tier.as_str(), slugify(provider_name, provider_id));
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_desktop_config::is_claude_safe_model_id;

    #[test]
    fn slugify_ascii_name() {
        assert_eq!(slugify("DeepSeek-OTN", "abc12345"), "deepseek-otn");
        assert_eq!(slugify("OpenCode Go", "abc12345"), "opencode-go");
    }

    #[test]
    fn slugify_uses_ascii_part_of_mixed_name() {
        // "智谱 GLM" 的 ASCII 部分是 GLM：非 ASCII 字符被跳过（不产生分隔符），
        // 得到可读的 slug —— 不要误以为"含中文就回落"
        assert_eq!(slugify("智谱 GLM", "97a1d0df-b9a9"), "glm");
    }

    #[test]
    fn slugify_falls_back_to_provider_id_when_no_ascii() {
        // 名称为纯非 ASCII 时 slug 为空，才回落为 provider id 前 8 位
        assert_eq!(slugify("月之暗面", "97a1d0df-b9a9"), "97a1d0df");
    }

    #[test]
    fn slugify_truncates_and_cleans_edges() {
        assert_eq!(slugify("  ---A--B---  ", "x"), "a-b");
        assert_eq!(slugify("abcdefghijklmnopqrstuvwxyz", "x").len(), 20);
    }

    #[test]
    fn generate_slot_id_is_claude_safe() {
        let id = generate_slot_id(AggregateTier::Sonnet, "DeepSeek-OTN", "pid", &[]);
        assert_eq!(id, "claude-sonnet-deepseek-otn");
        assert!(is_claude_safe_model_id(&id));
    }

    #[test]
    fn generate_slot_id_dedups_with_numeric_suffix() {
        let taken = vec!["claude-sonnet-glm".to_string()];
        let id = generate_slot_id(AggregateTier::Sonnet, "GLM", "pid", &taken);
        assert_eq!(id, "claude-sonnet-glm-2");
        assert!(is_claude_safe_model_id(&id));
    }
}
