//! 聚合供应商：无端点无凭据的虚拟供应商，按模型把请求分流到其他供应商。

use crate::claude_desktop_config::{is_claude_safe_model_id, ResolvedModelRoute};
use crate::error::AppError;
use crate::provider::Provider;
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

/// 一个槽位 = 一个可被 Claude 选择的模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateRouteSlot {
    /// 槽位 ID（**持久化**，是稳定的路由键）。
    /// 由前端在编辑时生成并随表单提交（UI 预览即提交值）；后端只校验它
    /// （非空、claude-safe、不重复），运行时不重新生成——这样重命名供应商
    /// 不会改变已生效的路由。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(route_id: &str, upstream_model: &str) -> AggregateRouteSlot {
        AggregateRouteSlot {
            route_id: route_id.to_string(),
            tier: AggregateTier::Sonnet,
            provider_id: "p-target".to_string(),
            upstream_model: upstream_model.to_string(),
            label: None,
            supports_1m: false,
        }
    }

    fn aggregate_provider(slots: Vec<AggregateRouteSlot>) -> Provider {
        let mut provider = Provider::with_id(
            "agg".to_string(),
            "Aggregate".to_string(),
            serde_json::json!({ "env": {} }),
            Some("https://example.com".to_string()),
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            aggregate_routes: Some(AggregateRoutes {
                slots,
                default_target: DefaultTarget::ProviderId("p-target".to_string()),
            }),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn aggregate_model_routes_filters_unsafe_route_ids() {
        // 不安全的 route_id 必须被丢弃（而非 repair 或原样发射）——
        // 一个坏 ID 会让 Claude Desktop 拒收整组 inferenceModels。
        // 同时保底：其余合法槽位仍应正常产出。
        let provider = aggregate_provider(vec![
            slot("claude-sonnet-1", "glm-5.3"),
            slot("glm-5.3", "some-upstream-model"),
        ]);

        let routes = aggregate_model_routes(&provider).expect("routes");

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].route_id, "claude-sonnet-1");
        assert_eq!(routes[0].upstream_model, "glm-5.3");
    }

    #[test]
    fn aggregate_model_routes_errors_when_all_slots_are_unusable() {
        // 全部槽位不可用时返回 Err（而不是安静地写出空列表 -> 空白模型选择器）
        let provider = aggregate_provider(vec![slot("glm-5.3", "some-upstream-model")]);

        assert!(aggregate_model_routes(&provider).is_err());
    }

    #[test]
    fn aggregate_model_routes_dedups_repeated_route_ids() {
        // 手工编辑产生的重复 route_id 只应产出一条
        let provider = aggregate_provider(vec![
            slot("claude-sonnet-1", "glm-a"),
            slot("claude-sonnet-1", "glm-b"),
        ]);

        let routes = aggregate_model_routes(&provider).expect("routes");

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].route_id, "claude-sonnet-1");
    }

    #[tokio::test]
    async fn resolve_target_hits_slot_by_generated_id() {
        let db = crate::database::Database::memory().expect("db");
        // 目标供应商（用仓库既有惯例 Provider::with_id 构造，Provider 未 derive Default）
        let target = crate::provider::Provider::with_id(
            "p-glm".to_string(),
            "GLM".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save target");

        let mut aggregate = crate::provider::Provider::with_id(
            "agg".to_string(),
            "Aggregate".to_string(),
            serde_json::json!({}),
            None,
        );
        aggregate.meta = Some(crate::provider::ProviderMeta {
            aggregate_routes: Some(AggregateRoutes {
                slots: vec![AggregateRouteSlot {
                    route_id: "claude-sonnet-1".into(),
                    tier: AggregateTier::Sonnet,
                    provider_id: "p-glm".into(),
                    upstream_model: "glm-5.3".into(),
                    label: None,
                    supports_1m: false,
                }],
                default_target: DefaultTarget::ProviderId("p-glm".into()),
            }),
            ..Default::default()
        });

        let hit = resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-1")
            .expect("hit");
        assert_eq!(hit.0.id, "p-glm");
        assert_eq!(hit.1.as_deref(), Some("glm-5.3"));

        // 未命中 → 默认目标，且不改写模型名
        let miss = resolve_target(&db, "claude-desktop", &aggregate, "claude-haiku-4-5")
            .expect("miss falls back to default");
        assert_eq!(miss.0.id, "p-glm");
        assert_eq!(miss.1, None);
    }

    /// 取出 `AppError::Localized` 的稳定 key（断言错误类型，而非本地化文案）。
    fn localized_key(err: &AppError) -> &'static str {
        match err {
            AppError::Localized { key, .. } => key,
            other => panic!("expected a localized error, got: {other}"),
        }
    }

    /// 槽位构造器：指向 `provider_id`。
    fn slot_for(
        route_id: &str,
        provider_id: &str,
        upstream_model: &str,
    ) -> AggregateRouteSlot {
        AggregateRouteSlot {
            route_id: route_id.to_string(),
            tier: AggregateTier::Sonnet,
            provider_id: provider_id.to_string(),
            upstream_model: upstream_model.to_string(),
            label: None,
            supports_1m: false,
        }
    }

    fn aggregate_with(slots: Vec<AggregateRouteSlot>, default_target: DefaultTarget) -> Provider {
        let mut aggregate = crate::provider::Provider::with_id(
            "agg".to_string(),
            "Aggregate".to_string(),
            serde_json::json!({}),
            None,
        );
        aggregate.meta = Some(crate::provider::ProviderMeta {
            aggregate_routes: Some(AggregateRoutes {
                slots,
                default_target,
            }),
            ..Default::default()
        });
        aggregate
    }

    #[tokio::test]
    async fn resolve_target_errors_when_default_target_slot_missing() {
        let db = crate::database::Database::memory().expect("db");
        // 默认目标指向一个不存在的槽位 id —— 必须显式报错，
        // **不得**静默回落到「第一个槽位」（那会把用户的兜底配置悄悄改掉）。
        let aggregate = aggregate_with(
            vec![slot_for("claude-sonnet-1", "p-glm", "glm-5.3")],
            DefaultTarget::SlotId("claude-sonnet-missing".into()),
        );

        let err = resolve_target(&db, "claude-desktop", &aggregate, "claude-haiku-4-5")
            .expect_err("default target pointing at a missing slot must error");
        assert_eq!(
            localized_key(&err),
            "aggregate.default_target_slot_missing"
        );
    }

    #[tokio::test]
    async fn resolve_target_errors_when_target_provider_missing() {
        let db = crate::database::Database::memory().expect("db");
        // 命中槽位，但它引用的目标供应商在库里不存在 → 明确错误
        let aggregate = aggregate_with(
            vec![slot_for("claude-sonnet-1", "p-missing", "glm-5.3")],
            DefaultTarget::ProviderId("p-missing".into()),
        );

        let err = resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-1")
            .expect_err("missing target provider must error");
        assert_eq!(localized_key(&err), "aggregate.target_provider_missing");
    }
}

/// 该供应商是否为聚合供应商。
pub fn is_aggregate_provider(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .is_some()
}

/// 由槽位派生模型规格（供 profile 的 inferenceModels 与 /models 端点共用）。
/// 槽位 ID 由前端在编辑时生成并持久化，这里直接取用；按 route_id 排序与既有实现保持一致。
pub fn aggregate_model_routes(provider: &Provider) -> Result<Vec<ResolvedModelRoute>, AppError> {
    let routes = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.routes_missing",
                "聚合供应商缺少路由表",
                "Aggregate provider is missing its route table",
            )
        })?;

    let mut out = Vec::with_capacity(routes.slots.len());
    for slot in &routes.slots {
        let upstream = slot.upstream_model.trim();
        let route_id = slot.route_id.trim();
        // route_id 是稳定路由键（路由表、DefaultTarget::SlotId、UI 均引用它），
        // 不做 repair —— 一个非 claude-safe 的 ID 会让 Claude Desktop 拒收整组
        // inferenceModels，故直接丢弃该槽位，保留其余合法槽位。
        if upstream.is_empty() || route_id.is_empty() || !is_claude_safe_model_id(route_id) {
            continue;
        }
        out.push(ResolvedModelRoute {
            route_id: route_id.to_string(),
            upstream_model: upstream.to_string(),
            label_override: slot
                .label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
            supports_1m: slot.supports_1m,
        });
    }
    out.sort_by(|a, b| a.route_id.cmp(&b.route_id));
    out.dedup_by(|a, b| a.route_id == b.route_id);

    if out.is_empty() {
        return Err(AppError::localized(
            "aggregate.routes_empty",
            "聚合供应商的路由表至少需要一个可用的模型槽位",
            "Aggregate provider requires at least one usable model route slot",
        ));
    }

    Ok(out)
}

/// 解析聚合路由：给定聚合供应商与请求模型，返回 (目标供应商, 需改写的上游模型名)。
/// - 命中槽位 → 返回该槽位的目标与上游模型
/// - 未命中 → 返回默认目标，模型名不改写（None）
/// - 默认目标也不可用 → 返回明确错误（不静默降级）
pub fn resolve_target(
    db: &crate::database::Database,
    app_type: &str,
    aggregate: &Provider,
    request_model: &str,
) -> Result<(Provider, Option<String>), AppError> {
    let routes = aggregate
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.routes_missing",
                "聚合供应商缺少路由表",
                "Aggregate provider is missing its route table",
            )
        })?;

    // 直接按持久化的槽位 ID 查表（ID 由前端编辑时生成并随表单提交，运行时不重新生成）
    for slot in &routes.slots {
        if slot.route_id == request_model {
            let target = load_provider(db, app_type, &slot.provider_id)?;
            return Ok((target, Some(slot.upstream_model.clone())));
        }
    }

    // 未命中 → 默认目标
    let fallback_id = match &routes.default_target {
        DefaultTarget::ProviderId(id) => id.clone(),
        DefaultTarget::SlotId(slot_id) => routes
            .slots
            .iter()
            .find(|slot| &slot.route_id == slot_id)
            .map(|slot| slot.provider_id.clone())
            .ok_or_else(|| {
                AppError::localized(
                    "aggregate.default_target_slot_missing",
                    "聚合供应商的默认目标指向了不存在的槽位",
                    "Aggregate default target points to a missing slot",
                )
            })?,
    };
    let target = load_provider(db, app_type, &fallback_id)?;
    Ok((target, None))
}

fn load_provider(
    db: &crate::database::Database,
    app_type: &str,
    provider_id: &str,
) -> Result<Provider, AppError> {
    db.get_provider_by_id(provider_id, app_type)?
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.target_provider_missing",
                "聚合供应商的目标供应商不存在",
                "Aggregate target provider does not exist",
            )
        })
}
