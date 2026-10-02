//! 聚合供应商：无端点无凭据的虚拟供应商，按模型把请求分流到其他供应商。

use crate::claude_desktop_config::{is_claude_safe_model_id, ResolvedModelRoute};
use crate::error::AppError;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};

/// 可写入 profile 的 `maxEffort` 合法取值（与 Claude Desktop 的 low…max 阶梯一致）。
/// 白名单外丢弃——避免用户被 Desktop 静默压到最低档（cap at low，实测 2.9939.4.0）。
const AGGREGATE_MAX_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

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
    /// 思考强度上限（camelCase: maxEffort）。仅完整阶梯 ID 有意义。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_effort: Option<String>,
}

/// 别名路由规则：请求名未命中任何槽位时，按前缀（大小写不敏感）转投目标槽位。
/// 按序先匹配先赢；空前缀/悬空槽位静默跳过（可选优化项失效，非配置性错误）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateAliasRule {
    pub prefix: String,
    pub slot_id: String,
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
    /// 默认模型（槽位 routeId）：写 profile 时置顶到 inferenceModels 首位
    /// —— Claude Desktop 认第一条为默认模型、启动新会话从它起步。
    /// None（含旧数据，serde default）= 跟随排序首位，行为同 v3 之前。
    #[serde(default)]
    pub default_model: Option<String>,
    /// 别名规则：槽位未命中时按序做前缀匹配（见 resolve_target）。旧数据无此键，
    /// serde default 反序列化为空列表 —— 行为等同未启用。
    #[serde(default)]
    pub alias_rules: Vec<AggregateAliasRule>,
}

/// Codex 聚合路由：客户端模型名（精确匹配）→ 目标 Codex 供应商 + 上游模型。
/// 与 Claude 侧 `AggregateRoutes` 平行、互不兼容（上游 PR #5937 占用 `aggregateRoutes`
/// 键，此处用独立键名避开碰撞）。数据面见 `resolve_codex_target`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAggregateRoutes {
    pub slots: Vec<CodexAggregateSlot>,
    pub default_target: DefaultTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAggregateSlot {
    /// 客户端模型名（slug，路由键）。非空与全表唯一由保存校验兜底。
    pub model: String,
    pub provider_id: String,
    pub upstream_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn slot(route_id: &str, upstream_model: &str) -> AggregateRouteSlot {
        AggregateRouteSlot {
            route_id: route_id.to_string(),
            tier: AggregateTier::Sonnet,
            provider_id: "p-target".to_string(),
            upstream_model: upstream_model.to_string(),
            label: None,
            supports_1m: false,
            max_effort: None,
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
                default_model: None,
                alias_rules: vec![],
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

    #[test]
    fn aggregate_model_routes_preserves_provider_grouped_order() {
        // profile 的 inferenceModels 顺序 = Claude Desktop 选择器顺序。
        // UI 提交时已按「供应商分组 × fable→opus→sonnet→haiku」展平 slots，后端必须
        // 原样保留——按 route_id 字典序重排会把不同供应商的模型交错穿插
        // （用户 2026-09-24 反馈：Zhipu/DeepSeek/Ark/OpenCode 的模型混在一起）。
        let grouped = |provider_id: &str, tier: AggregateTier, route_id: &str, model: &str| {
            slot_for(route_id, provider_id, tier, model)
        };
        let provider = aggregate_provider(vec![
            grouped("p-zhipu", AggregateTier::Fable, "claude-fable-1", "glm-5.3"),
            grouped(
                "p-zhipu",
                AggregateTier::Opus,
                "claude-opus-4-8",
                "glm-5.3-flash",
            ),
            grouped(
                "p-ds",
                AggregateTier::Fable,
                "claude-fable-2",
                "deepseek-v4-pro",
            ),
            grouped(
                "p-ds",
                AggregateTier::Opus,
                "claude-opus-4-7",
                "deepseek-flash",
            ),
            grouped("p-ark", AggregateTier::Fable, "claude-fable-3", "kimi-k3"),
            grouped(
                "p-oc",
                AggregateTier::Sonnet,
                "claude-sonnet-4-5",
                "space-bunny-free",
            ),
        ]);

        let routes = aggregate_model_routes(&provider).expect("routes");

        let order: Vec<&str> = routes.iter().map(|r| r.route_id.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "claude-fable-1",    // Zhipu
                "claude-opus-4-8",   // Zhipu
                "claude-fable-2",    // DeepSeek
                "claude-opus-4-7",   // DeepSeek
                "claude-fable-3",    // Ark
                "claude-sonnet-4-5", // OpenCode
            ],
            "必须保持供应商分组顺序，不能按 route_id 字典序重排"
        );
        // 字典序会把 sonnet 排到 opus 之前，正是要避免的交错
        let mut lexical = order.clone();
        lexical.sort_unstable();
        assert_ne!(order, lexical, "本用例应能区分分组序与字典序");
    }

    #[test]
    fn aggregate_model_routes_carries_tier_for_ordering() {
        // tier 随槽位带出（供 profile 写入侧按档位强弱呈现），普通供应商路径恒为 None
        let provider = aggregate_provider(vec![slot("claude-fable-1", "glm-5.3")]);
        let routes = aggregate_model_routes(&provider).expect("routes");
        assert_eq!(routes[0].tier.as_deref(), Some("sonnet")); // fixture 默认 sonnet
    }

    #[test]
    fn aggregate_model_routes_pins_default_model_to_front() {
        // defaultModel 命中槽位 → 该槽置顶（inferenceModels 第一条 = Claude Desktop
        // 的默认模型），其余保持供应商分组序
        let grouped = |provider_id: &str, tier: AggregateTier, route_id: &str, model: &str| {
            slot_for(route_id, provider_id, tier, model)
        };
        let slots = vec![
            grouped("p-zhipu", AggregateTier::Fable, "claude-fable-1", "glm-5.3"),
            grouped(
                "p-ds",
                AggregateTier::Fable,
                "claude-fable-2",
                "deepseek-v4-pro",
            ),
            grouped(
                "p-ds",
                AggregateTier::Opus,
                "claude-opus-4-7",
                "deepseek-flash",
            ),
        ];
        let mut provider = aggregate_provider(slots);
        if let Some(routes) = provider.meta.as_mut().unwrap().aggregate_routes.as_mut() {
            routes.default_model = Some("claude-opus-4-7".to_string());
        }

        let routes = aggregate_model_routes(&provider).expect("routes");
        let order: Vec<&str> = routes.iter().map(|r| r.route_id.as_str()).collect();
        assert_eq!(
            order,
            vec!["claude-opus-4-7", "claude-fable-1", "claude-fable-2"]
        );
    }

    #[test]
    fn aggregate_model_routes_ignores_dangling_default_model() {
        // 引用已删除的槽位 → 静默忽略、顺序不变（只影响启动默认，不影响路由）
        let provider = aggregate_provider(vec![
            slot("claude-sonnet-1", "glm-5.3"),
            slot("claude-sonnet-2", "glm-5.3-flash"),
        ]);
        let mut provider = provider;
        if let Some(routes) = provider.meta.as_mut().unwrap().aggregate_routes.as_mut() {
            routes.default_model = Some("claude-sonnet-gone".to_string());
        }

        let routes = aggregate_model_routes(&provider).expect("routes");
        let order: Vec<&str> = routes.iter().map(|r| r.route_id.as_str()).collect();
        assert_eq!(order, vec!["claude-sonnet-1", "claude-sonnet-2"]);
    }

    #[test]
    fn aggregate_routes_deserializes_without_default_model() {
        // 旧数据无 defaultModel 字段 → serde default 反序列化为 None，零迁移
        let legacy = serde_json::json!({
            "slots": [{
                "routeId": "claude-sonnet-1", "tier": "sonnet",
                "providerId": "p1", "upstreamModel": "glm-5.3",
            }],
            "defaultTarget": {"kind": "providerId", "value": "p1"},
        });
        let routes: AggregateRoutes = serde_json::from_value(legacy).expect("legacy json");
        assert!(routes.default_model.is_none());
        assert_eq!(routes.slots.len(), 1);
    }

    #[test]
    fn max_effort_absent_defaults_to_none_and_survives_roundtrip() {
        let plain: AggregateRouteSlot = serde_json::from_str(
            r#"{"routeId":"claude-sonnet-5","tier":"sonnet","providerId":"p","upstreamModel":"m","supports1m":false}"#,
        )
        .unwrap();
        assert_eq!(plain.max_effort, None);

        let with: AggregateRouteSlot = serde_json::from_str(
            r#"{"routeId":"claude-sonnet-5","tier":"sonnet","providerId":"p","upstreamModel":"m","supports1m":false,"maxEffort":"xhigh"}"#,
        )
        .unwrap();
        assert_eq!(with.max_effort.as_deref(), Some("xhigh"));
        assert!(serde_json::to_string(&with)
            .unwrap()
            .contains(r#""maxEffort":"xhigh""#));
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
        db.save_provider("claude-desktop", &target)
            .expect("save target");

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
                    max_effort: None,
                }],
                default_target: DefaultTarget::ProviderId("p-glm".into()),
                default_model: None,
                alias_rules: vec![],
            }),
            ..Default::default()
        });

        let hit =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-1").expect("hit");
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

    /// 槽位构造器：指向 `provider_id`、按 `tier` 档位。
    fn slot_for(
        route_id: &str,
        provider_id: &str,
        tier: AggregateTier,
        upstream_model: &str,
    ) -> AggregateRouteSlot {
        AggregateRouteSlot {
            route_id: route_id.to_string(),
            tier,
            provider_id: provider_id.to_string(),
            upstream_model: upstream_model.to_string(),
            label: None,
            supports_1m: false,
            max_effort: None,
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
                default_model: None,
                alias_rules: vec![],
            }),
            ..Default::default()
        });
        aggregate
    }

    /// 给聚合供应商设置别名规则（测试辅助）。`aggregate_with` 返回 `Provider`，
    /// 故本辅助同样进出 `Provider`。
    fn with_alias_rules(mut aggregate: Provider, rules: Vec<AggregateAliasRule>) -> Provider {
        aggregate
            .meta
            .as_mut()
            .and_then(|meta| meta.aggregate_routes.as_mut())
            .expect("aggregate routes")
            .alias_rules = rules;
        aggregate
    }

    #[tokio::test]
    async fn resolve_target_errors_when_default_target_slot_missing() {
        let db = crate::database::Database::memory().expect("db");
        // 默认目标指向一个不存在的槽位 id —— 必须显式报错，
        // **不得**静默回落到「第一个槽位」（那会把用户的兜底配置悄悄改掉）。
        let aggregate = aggregate_with(
            vec![slot_for(
                "claude-sonnet-1",
                "p-glm",
                AggregateTier::Sonnet,
                "glm-5.3",
            )],
            DefaultTarget::SlotId("claude-sonnet-missing".into()),
        );

        let err = resolve_target(&db, "claude-desktop", &aggregate, "claude-haiku-4-5")
            .expect_err("default target pointing at a missing slot must error");
        assert_eq!(localized_key(&err), "aggregate.default_target_slot_missing");
    }

    #[tokio::test]
    async fn resolve_target_errors_when_target_provider_missing() {
        let db = crate::database::Database::memory().expect("db");
        // 命中槽位，但它引用的目标供应商在库里不存在 → 明确错误
        let aggregate = aggregate_with(
            vec![slot_for(
                "claude-sonnet-1",
                "p-missing",
                AggregateTier::Sonnet,
                "glm-5.3",
            )],
            DefaultTarget::ProviderId("p-missing".into()),
        );

        let err = resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-1")
            .expect_err("missing target provider must error");
        assert_eq!(localized_key(&err), "aggregate.target_provider_missing");
    }

    /// 1M 变体：槽位开了 `supports1m` 后，Claude Desktop 会生成 `<槽位 ID>[1m]` 的
    /// 模型条目，用户选中它时请求里的 model 就带这个后缀。路由查找必须先剥离该标记，
    /// 否则整批 1M 请求都会漏到默认目标，再由**目标供应商自己的路由表**改写模型名
    /// ——表现为「选了 A 家的 1M 变体，实际打到 B 家的模型」（2026-09-23 本机实测：
    /// `claude-fable-1[1m]` 被打到 DeepSeek 的 `deepseek-v4-pro`，而该槽位是智谱的
    /// `glm-5.3`；默认目标写成另一家供应商，漏过去就会被断言抓到）。
    #[tokio::test]
    async fn resolve_target_strips_one_m_marker_before_slot_lookup() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-glm".to_string(),
            "GLM".to_string(),
            serde_json::json!({}),
            None,
        );
        let fallback = crate::provider::Provider::with_id(
            "p-other".to_string(),
            "Other".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target)
            .expect("save target");
        db.save_provider("claude-desktop", &fallback)
            .expect("save fallback");

        let aggregate = aggregate_with(
            vec![slot_for(
                "claude-fable-1",
                "p-glm",
                AggregateTier::Sonnet,
                "glm-5.3",
            )],
            DefaultTarget::ProviderId("p-other".into()),
        );

        for requested in [
            "claude-fable-1[1m]",
            "claude-fable-1[1M] ",
            "claude-fable-1 [1m]",
        ] {
            let hit = resolve_target(&db, "claude-desktop", &aggregate, requested)
                .unwrap_or_else(|e| panic!("{requested} 应命中槽位: {e:?}"));
            assert_eq!(hit.0.id, "p-glm", "{requested} 不应漏到默认目标");
            assert_eq!(hit.1.as_deref(), Some("glm-5.3"), "{requested}");
        }
    }

    #[tokio::test]
    async fn alias_prefix_hits_route_to_slot_upstream() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-oc".to_string(),
            "OpenCode".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-oc",
                    AggregateTier::Sonnet,
                    "space-bunny-free",
                )],
                DefaultTarget::ProviderId("p-oc".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-sonnet-4".into(),
            }],
        );
        // 平台别名不在槽位表 → 命中前缀 → 直给槽位上游模型
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5")
                .expect("alias hit");
        assert_eq!(prov.id, "p-oc");
        assert_eq!(upstream.as_deref(), Some("space-bunny-free"));
    }

    #[tokio::test]
    async fn alias_matching_is_case_insensitive() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-oc".to_string(),
            "OpenCode".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-oc",
                    AggregateTier::Sonnet,
                    "space-bunny-free",
                )],
                DefaultTarget::ProviderId("p-oc".into()),
            ),
            vec![AggregateAliasRule {
                // 混合大小写：前缀侧与请求侧都必须小写化才能命中
                prefix: "Claude-SONNET".into(),
                slot_id: "claude-sonnet-4".into(),
            }],
        );
        let (_, upstream) = resolve_target(&db, "claude-desktop", &aggregate, "CLAUDE-SONNET-5-5")
            .expect("case-insensitive alias hit");
        assert_eq!(upstream.as_deref(), Some("space-bunny-free"));
    }

    #[tokio::test]
    async fn alias_request_with_1m_suffix_still_matches() {
        let db = crate::database::Database::memory().expect("db");
        let target = crate::provider::Provider::with_id(
            "p-oc".to_string(),
            "OpenCode".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &target).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-oc",
                    AggregateTier::Sonnet,
                    "space-bunny-free",
                )],
                DefaultTarget::ProviderId("p-oc".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-sonnet-4".into(),
            }],
        );
        // [1m] 后缀在匹配前已被 strip_one_m_suffix_for_route_lookup 剥掉
        let (_, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5[1m]")
                .expect("alias hit after 1m strip");
        assert_eq!(upstream.as_deref(), Some("space-bunny-free"));
    }

    #[tokio::test]
    async fn alias_first_match_wins() {
        let db = crate::database::Database::memory().expect("db");
        for (id, name) in [("p-a", "A"), ("p-b", "B")] {
            let p = crate::provider::Provider::with_id(
                id.to_string(),
                name.to_string(),
                serde_json::json!({}),
                None,
            );
            db.save_provider("claude-desktop", &p).expect("save");
        }
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![
                    slot_for("claude-sonnet-4", "p-a", AggregateTier::Sonnet, "model-a"),
                    slot_for("claude-sonnet-2", "p-b", AggregateTier::Sonnet, "model-b"),
                ],
                DefaultTarget::ProviderId("p-b".into()),
            ),
            // 两条规则都命中 "claude-sonnet-5-5"：靠前者赢
            vec![
                AggregateAliasRule {
                    prefix: "claude-sonnet".into(),
                    slot_id: "claude-sonnet-4".into(),
                },
                AggregateAliasRule {
                    prefix: "claude-".into(),
                    slot_id: "claude-sonnet-2".into(),
                },
            ],
        );
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5")
                .expect("first rule wins");
        assert_eq!(prov.id, "p-a");
        assert_eq!(upstream.as_deref(), Some("model-a"));
    }

    #[tokio::test]
    async fn alias_dangling_slot_skipped_falls_to_default() {
        let db = crate::database::Database::memory().expect("db");
        let fallback = crate::provider::Provider::with_id(
            "p-fallback".to_string(),
            "Fallback".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &fallback).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-fallback",
                    AggregateTier::Sonnet,
                    "model-x",
                )],
                DefaultTarget::ProviderId("p-fallback".into()),
            ),
            vec![AggregateAliasRule {
                prefix: "claude-sonnet".into(),
                slot_id: "claude-gone".into(),
            }],
        );
        // 悬空规则跳过 → 走兜底，且兜底不改写模型名（upstream = None）
        let (prov, upstream) =
            resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-5-5")
                .expect("dangling alias must not error");
        assert_eq!(prov.id, "p-fallback");
        assert_eq!(upstream, None);
    }

    #[tokio::test]
    async fn alias_empty_or_whitespace_prefix_skipped() {
        let db = crate::database::Database::memory().expect("db");
        let fallback = crate::provider::Provider::with_id(
            "p-fallback".to_string(),
            "Fallback".to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude-desktop", &fallback).expect("save");
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![slot_for(
                    "claude-sonnet-4",
                    "p-fallback",
                    AggregateTier::Sonnet,
                    "model-x",
                )],
                DefaultTarget::ProviderId("p-fallback".into()),
            ),
            vec![
                AggregateAliasRule {
                    prefix: "".into(),
                    slot_id: "claude-sonnet-4".into(),
                },
                AggregateAliasRule {
                    prefix: "   ".into(),
                    slot_id: "claude-sonnet-4".into(),
                },
            ],
        );
        let (prov, upstream) = resolve_target(&db, "claude-desktop", &aggregate, "anything")
            .expect("empty prefixes skipped");
        assert_eq!(prov.id, "p-fallback");
        assert_eq!(upstream, None);
    }

    #[tokio::test]
    async fn exact_slot_match_wins_over_alias() {
        let db = crate::database::Database::memory().expect("db");
        for (id, name) in [("p-a", "A"), ("p-b", "B")] {
            let p = crate::provider::Provider::with_id(
                id.to_string(),
                name.to_string(),
                serde_json::json!({}),
                None,
            );
            db.save_provider("claude-desktop", &p).expect("save");
        }
        let aggregate = with_alias_rules(
            aggregate_with(
                vec![
                    slot_for("claude-sonnet-4", "p-a", AggregateTier::Sonnet, "model-a"),
                    slot_for("claude-haiku-2", "p-b", AggregateTier::Sonnet, "model-b"),
                ],
                DefaultTarget::ProviderId("p-b".into()),
            ),
            // 前缀足以吞掉真槽 ID "claude-sonnet-4"，但精确命中优先
            vec![AggregateAliasRule {
                prefix: "claude-sonnet-4".into(),
                slot_id: "claude-haiku-2".into(),
            }],
        );
        let (prov, upstream) = resolve_target(&db, "claude-desktop", &aggregate, "claude-sonnet-4")
            .expect("exact slot wins");
        assert_eq!(prov.id, "p-a");
        assert_eq!(upstream.as_deref(), Some("model-a"));
    }

    #[test]
    fn alias_rules_absent_in_old_json_deserializes_empty() {
        let json = r#"{"slots":[],"defaultTarget":{"kind":"providerId","value":"p"}}"#;
        let routes: AggregateRoutes = serde_json::from_str(json).unwrap();
        assert!(routes.alias_rules.is_empty());

        let with = r#"{"slots":[],"defaultTarget":{"kind":"providerId","value":"p"},"aliasRules":[{"prefix":"claude-sonnet","slotId":"claude-sonnet-4"}]}"#;
        let routes: AggregateRoutes = serde_json::from_str(with).unwrap();
        assert_eq!(routes.alias_rules.len(), 1);
        assert_eq!(routes.alias_rules[0].prefix, "claude-sonnet");
        assert_eq!(routes.alias_rules[0].slot_id, "claude-sonnet-4");
    }

    fn codex_aggregate_provider(routes: Option<CodexAggregateRoutes>) -> Provider {
        let mut provider = Provider::with_id(
            "agg-codex".to_string(),
            "Codex Aggregate".to_string(),
            serde_json::json!({}),
            None,
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            codex_aggregate_routes: routes,
            ..Default::default()
        });
        provider
    }

    #[test]
    fn codex_routes_of_errors_when_missing() {
        // 没有 codexAggregateRoutes 的供应商不是聚合供应商，取路由表必须明确报错
        // （不得回落成 Claude 侧的 aggregateRoutes —— 两套键互不相通）
        let plain = codex_aggregate_provider(None);
        assert!(!is_codex_aggregate_provider(&plain));
        let err = codex_routes_of(&plain).expect_err("missing route table must error");
        assert_eq!(localized_key(&err), "codex_aggregate.routes_missing");

        let with_routes = codex_aggregate_provider(Some(CodexAggregateRoutes {
            slots: vec![CodexAggregateSlot {
                model: "gpt-5.1".to_string(),
                provider_id: "p-kimi".to_string(),
                upstream_model: "kimi-k2".to_string(),
                label: None,
            }],
            default_target: DefaultTarget::ProviderId("p-kimi".to_string()),
            default_model: None,
        }));
        assert!(is_codex_aggregate_provider(&with_routes));
        let routes = codex_routes_of(&with_routes).expect("route table");
        assert_eq!(routes.slots[0].upstream_model, "kimi-k2");
    }

    /// Codex 槽位构造器：客户端模型名 → (目标供应商, 上游模型)。
    fn codex_slot(model: &str, provider_id: &str, upstream_model: &str) -> CodexAggregateSlot {
        CodexAggregateSlot {
            model: model.to_string(),
            provider_id: provider_id.to_string(),
            upstream_model: upstream_model.to_string(),
            label: None,
        }
    }

    fn codex_aggregate_with(
        slots: Vec<CodexAggregateSlot>,
        default_target: DefaultTarget,
    ) -> Provider {
        codex_aggregate_provider(Some(CodexAggregateRoutes {
            slots,
            default_target,
            default_model: None,
        }))
    }

    /// 保存一个 Codex 目标供应商（端点/凭据不在本组用例的断言范围内）。
    fn save_codex_target(db: &crate::database::Database, id: &str) {
        let provider = crate::provider::Provider::with_id(
            id.to_string(),
            id.to_string(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("codex", &provider)
            .expect("save codex target");
    }

    #[tokio::test]
    async fn resolve_codex_target_exact_match_rewrites_model() {
        let db = crate::database::Database::memory().expect("db");
        save_codex_target(&db, "p-kimi");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-kimi", "kimi-k2")],
            DefaultTarget::ProviderId("p-kimi".into()),
        );

        let (target, upstream) =
            resolve_codex_target(&db, "codex", &aggregate, "gpt-5.1").expect("exact slot hit");
        assert_eq!(target.id, "p-kimi");
        assert_eq!(upstream.as_deref(), Some("kimi-k2"));
    }

    #[tokio::test]
    async fn resolve_codex_target_falls_back_to_default_provider_without_rewrite() {
        let db = crate::database::Database::memory().expect("db");
        save_codex_target(&db, "p-kimi");
        save_codex_target(&db, "p-other");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-kimi", "kimi-k2")],
            DefaultTarget::ProviderId("p-other".into()),
        );

        let (target, upstream) =
            resolve_codex_target(&db, "codex", &aggregate, "gpt-9.9").expect("miss falls back");
        assert_eq!(target.id, "p-other");
        assert_eq!(upstream, None, "兜底不改写模型名");
    }

    #[tokio::test]
    async fn resolve_codex_target_default_slot_rewrites_to_slot_upstream() {
        let db = crate::database::Database::memory().expect("db");
        save_codex_target(&db, "p-kimi");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-kimi", "kimi-k2")],
            DefaultTarget::SlotId("gpt-5.1".into()),
        );

        // 与 Claude 侧同款：SlotId 兜底保留该槽的上游模型名（槽位本身就是一份路由）
        let (target, upstream) =
            resolve_codex_target(&db, "codex", &aggregate, "gpt-9.9").expect("slot fallback");
        assert_eq!(target.id, "p-kimi");
        assert_eq!(upstream.as_deref(), Some("kimi-k2"));
    }

    #[tokio::test]
    async fn resolve_codex_target_dangling_default_slot_errors() {
        let db = crate::database::Database::memory().expect("db");
        save_codex_target(&db, "p-kimi");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-kimi", "kimi-k2")],
            DefaultTarget::SlotId("gpt-missing".into()),
        );

        let err = resolve_codex_target(&db, "codex", &aggregate, "gpt-9.9")
            .expect_err("default target pointing at a missing slot must error");
        assert_eq!(
            localized_key(&err),
            "codex_aggregate.default_target_slot_missing"
        );
    }

    #[tokio::test]
    async fn resolve_codex_target_errors_when_target_provider_missing() {
        let db = crate::database::Database::memory().expect("db");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-missing", "kimi-k2")],
            DefaultTarget::ProviderId("p-missing".into()),
        );

        let err = resolve_codex_target(&db, "codex", &aggregate, "gpt-5.1")
            .expect_err("missing target provider must error");
        assert_eq!(
            localized_key(&err),
            "codex_aggregate.target_provider_missing"
        );
    }

    #[tokio::test]
    async fn resolve_codex_target_strips_1m_marker_before_lookup() {
        let db = crate::database::Database::memory().expect("db");
        save_codex_target(&db, "p-kimi");
        save_codex_target(&db, "p-other");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-kimi", "kimi-k2")],
            DefaultTarget::ProviderId("p-other".into()),
        );

        // 客户端可能带 1M 能力标记发请求；不剥离就会整批漏到兜底，再由目标供应商
        // 自己的路由表改写模型名（与 Claude 侧 2026-09-23 同款事故）。
        for requested in ["gpt-5.1[1M]", "gpt-5.1[1m]", "gpt-5.1 [1m]"] {
            let (target, upstream) = resolve_codex_target(&db, "codex", &aggregate, requested)
                .unwrap_or_else(|e| panic!("{requested} 应命中槽位: {e:?}"));
            assert_eq!(target.id, "p-kimi", "{requested} 不应漏到兜底");
            assert_eq!(upstream.as_deref(), Some("kimi-k2"), "{requested}");
        }
    }

    #[tokio::test]
    async fn resolve_codex_target_requires_exact_model_match() {
        // Codex 侧没有别名层：前缀相近的模型名不得误命中（否则会把用户请求发到
        // 一家他没选的供应商上）。
        let db = crate::database::Database::memory().expect("db");
        save_codex_target(&db, "p-kimi");
        save_codex_target(&db, "p-other");
        let aggregate = codex_aggregate_with(
            vec![codex_slot("gpt-5.1", "p-kimi", "kimi-k2")],
            DefaultTarget::ProviderId("p-other".into()),
        );

        for requested in ["gpt-5.10", "gpt-5", "GPT-5.1", "gpt-5.1-codex"] {
            let (target, upstream) =
                resolve_codex_target(&db, "codex", &aggregate, requested).expect("fallback");
            assert_eq!(target.id, "p-other", "{requested} 必须走兜底");
            assert_eq!(upstream, None, "{requested}");
        }
    }

    /// 合成用的三槽路由表：默认模型 = 第 3 槽（与 Claude 侧用例同款构造）。
    fn codex_synthesis_routes() -> CodexAggregateRoutes {
        CodexAggregateRoutes {
            slots: vec![
                codex_slot("gpt-5.1", "p-kimi", "kimi-k2"),
                codex_slot("glm-5.3", "p-zhipu", "glm-5.3"),
                codex_slot("kimi-k3", "p-ark", "kimi-k3-instruct"),
            ],
            default_target: DefaultTarget::ProviderId("p-kimi".into()),
            default_model: Some("kimi-k3".into()),
        }
    }

    #[test]
    fn codex_aggregate_seed_toml_places_fields_correctly() {
        let routes = codex_synthesis_routes();
        let settings = synthesize_codex_aggregate_settings(&routes, "http://127.0.0.1:15721")
            .expect("settings");
        let config = settings["config"]
            .as_str()
            .expect("config is a TOML string");
        let header = config
            .find("[model_providers.cc-switch-aggregate]")
            .expect("seed provider table");

        // 顶层字段必须在表头之前：Codex CLI 只在文档根部读它们，写进表里等于没写
        let model_line = config
            .find("model = \"kimi-k3\"")
            .expect("top-level model line");
        assert!(model_line < header, "顶层 model 必须在表头之前:\n{config}");
        let disable_storage = config
            .find("disable_response_storage = true")
            .expect("top-level disable_response_storage");
        assert!(
            disable_storage < header,
            "顶层 disable_response_storage 必须在表头之前:\n{config}"
        );

        // 表内字段必须在表头之后
        for inside in [
            "base_url = \"http://127.0.0.1:15721\"",
            "wire_api = \"responses\"",
            "name = \"cc-switch Aggregate\"",
            "requires_openai_auth = true",
        ] {
            let at = config
                .find(inside)
                .unwrap_or_else(|| panic!("缺少 {inside}:\n{config}"));
            assert!(at > header, "{inside} 必须写在表内:\n{config}");
        }

        // base_url 是代理 origin 根（无 /claude-desktop 尾巴）：/v1/responses 挂在根路由
        assert!(config.contains("model_provider = \"cc-switch-aggregate\""));
        assert!(!config.contains("/claude-desktop"));
        // 顶层 model 取 defaultModel 命中的槽位
        assert!(!config.contains("model = \"gpt-5.1\""));

        assert_eq!(settings["auth"]["OPENAI_API_KEY"], json!("PROXY_MANAGED"));
        assert_eq!(
            settings["modelCatalog"]["models"]
                .as_array()
                .expect("catalog models")
                .len(),
            3
        );
    }

    #[test]
    fn codex_aggregate_seed_toml_falls_back_to_first_slot_model() {
        // defaultModel 缺失/悬空 → 顶层 model 取槽位序首位（与 Claude 侧同款）
        let mut routes = codex_synthesis_routes();
        routes.default_model = None;
        let settings = synthesize_codex_aggregate_settings(&routes, "http://127.0.0.1:15721")
            .expect("settings");
        assert!(settings["config"]
            .as_str()
            .expect("config")
            .contains("model = \"gpt-5.1\""));

        routes.default_model = Some("gpt-missing".into());
        let dangling = synthesize_codex_aggregate_settings(&routes, "http://127.0.0.1:15721")
            .expect("dangling default must not error");
        assert!(dangling["config"]
            .as_str()
            .expect("config")
            .contains("model = \"gpt-5.1\""));
    }

    #[test]
    fn codex_aggregate_catalog_preserves_slot_order_and_pins_default() {
        let routes = codex_synthesis_routes();
        let catalog = codex_aggregate_model_catalog(&routes);

        let slugs: Vec<&str> = catalog
            .iter()
            .map(|entry| entry["model"].as_str().expect("model slug"))
            .collect();
        assert_eq!(
            slugs,
            vec!["kimi-k3", "gpt-5.1", "glm-5.3"],
            "defaultModel 命中的槽位置顶，其余保持槽位序"
        );
        // 无 label 的槽不写 displayName 键（回填/保存时靠它区分「用户留空」与「显式清空」）
        assert!(catalog[1].get("displayName").is_none());
    }

    #[test]
    fn codex_aggregate_catalog_keeps_non_empty_label_as_display_name() {
        let mut routes = codex_synthesis_routes();
        routes.slots[0].label = Some("  Kimi K2 (Moonshot)  ".to_string());
        routes.slots[1].label = Some("   ".to_string());

        let catalog = codex_aggregate_model_catalog(&routes);
        // defaultModel 命中置顶，按 model 查条目（槽位下标与目录下标不一致）
        let entry = |model: &str| {
            catalog
                .iter()
                .find(|entry| entry["model"].as_str() == Some(model))
                .unwrap_or_else(|| panic!("catalog 缺少 {model}"))
        };
        assert_eq!(entry("gpt-5.1")["displayName"], json!("Kimi K2 (Moonshot)"));
        assert!(
            entry("glm-5.3").get("displayName").is_none(),
            "纯空白 label 视同留空"
        );
    }

    #[test]
    fn codex_aggregate_synthesis_skips_half_finished_slots() {
        // 半成品槽位（模型名/上游模型留空 = 用户正编辑中）必须跳过，不得产出会让
        // Codex 拿到空 model 的条目；与 Claude 侧 aggregate_model_routes 的
        // trim+过滤同款。
        let routes = CodexAggregateRoutes {
            slots: vec![
                codex_slot("  ", "p-kimi", "kimi-k2"),
                codex_slot("glm-5.3", "p-zhipu", "   "),
                codex_slot("kimi-k3", "p-ark", "kimi-k3-instruct"),
            ],
            default_target: DefaultTarget::ProviderId("p-kimi".into()),
            default_model: Some("glm-5.3".into()),
        };

        let catalog = codex_aggregate_model_catalog(&routes);
        let slugs: Vec<&str> = catalog
            .iter()
            .map(|entry| entry["model"].as_str().expect("model slug"))
            .collect();
        assert_eq!(slugs, vec!["kimi-k3"]);

        // 被过滤掉的 defaultModel 不得留下悬空顶层 model
        let settings = synthesize_codex_aggregate_settings(&routes, "http://127.0.0.1:15721")
            .expect("settings");
        assert!(settings["config"]
            .as_str()
            .expect("config")
            .contains("model = \"kimi-k3\""));
    }

    #[test]
    fn codex_aggregate_synthesis_errors_when_all_slots_are_unusable() {
        // 全部槽位不可用 → 明确报错（而不是安静地写出没有模型的空目录）
        let routes = CodexAggregateRoutes {
            slots: vec![codex_slot(" ", "p-kimi", " ")],
            default_target: DefaultTarget::ProviderId("p-kimi".into()),
            default_model: None,
        };

        let err = synthesize_codex_aggregate_settings(&routes, "http://127.0.0.1:15721")
            .expect_err("all slots unusable must error");
        assert_eq!(localized_key(&err), "codex_aggregate.routes_empty");
    }
}

/// seed `config.toml` 里的 provider 表 id。Codex CLI 读它作为 `model_provider`
/// 的取值；刻意不用 `custom` 等通用名，避免与用户自己手写的同名表相撞。
pub const CODEX_AGGREGATE_PROVIDER_ID: &str = "cc-switch-aggregate";
/// seed `config.toml` 里的表显示名。
const CODEX_AGGREGATE_PROVIDER_NAME: &str = "cc-switch Aggregate";
/// auth 占位值：聚合自身无凭据，真实 token 由本地代理按目标供应商注入。
/// 与代理接管写入的占位字面量一致（codex_config::CODEX_PROXY_AUTH_PLACEHOLDER
/// 同值，但那是私有常量，这里只能按字面量写——见全局约束「字面量逐字」）。
const CODEX_AGGREGATE_AUTH_PLACEHOLDER: &str = "PROXY_MANAGED";

/// 从 Codex 聚合路由表派生客户端可见的模型条目（`[{model, displayName?}]`）。
/// - 半成品槽位（模型名或上游模型留空 = 用户正编辑）跳过，与 Claude 侧
///   `aggregate_model_routes` 的 trim+过滤同款。
/// - `defaultModel` 命中则该槽位置顶——Codex CLI 的默认模型取目录第一条。
/// - `label` trim 后非空才写 `displayName`，留空时回落到 slug。
///
/// 落盘成 `models_cache.json` 形状是既有管线（`write_codex_provider_live_with_catalog`
/// → `prepare_codex_config_text_with_model_catalog`）的职责，这里只产出 settings 级形状。
pub fn codex_aggregate_model_catalog(routes: &CodexAggregateRoutes) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = Vec::with_capacity(routes.slots.len());
    for slot in &routes.slots {
        let model = slot.model.trim();
        if model.is_empty() || slot.upstream_model.trim().is_empty() {
            continue;
        }
        let mut entry = serde_json::Map::new();
        entry.insert(
            "model".to_string(),
            serde_json::Value::String(model.to_string()),
        );
        if let Some(label) = slot
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
        {
            entry.insert(
                "displayName".to_string(),
                serde_json::Value::String(label.to_string()),
            );
        }
        out.push(serde_json::Value::Object(entry));
    }

    // 默认模型置顶；引用悬空（槽已删/改名）时静默忽略、保持原序。
    if let Some(default_model) = routes.default_model.as_deref().map(str::trim) {
        if !default_model.is_empty() {
            if let Some(pos) = out
                .iter()
                .position(|entry| entry["model"].as_str() == Some(default_model))
            {
                let pinned = out.remove(pos);
                out.insert(0, pinned);
            }
        }
    }

    out
}

/// 由 Codex 聚合路由表合成写 live 用的有效配置（`{auth, config, modelCatalog}`）。
///
/// 聚合供应商按设计无端点无凭据（`settings_config` 是空对象），而 Codex live 写入
/// 要求 `auth` 是 JSON 对象、`config` 是 TOML 字符串——本函数在**有效快照**里满足
/// 这两个校验，DB 行保持空（与 Claude Desktop 聚合的「无端点无凭据」一致）。
///
/// `proxy_origin` 是代理 origin **根**（`http://127.0.0.1:15721`），由运行期代理
/// 配置派生、不硬编码；Codex 的 `/v1/responses` 挂在根路由上，因此不带
/// `/claude-desktop` 这类应用前缀（那串前缀只属于 Claude Desktop 网关）。
///
/// seed 的字段位置是承重的：Codex CLI 只在文档根部读 `model` / `model_provider` /
/// `disable_response_storage`，写进 `[model_providers.*]` 表里等于没写。
pub fn synthesize_codex_aggregate_settings(
    routes: &CodexAggregateRoutes,
    proxy_origin: &str,
) -> Result<serde_json::Value, AppError> {
    let catalog = codex_aggregate_model_catalog(routes);
    let Some(default_entry) = catalog.first() else {
        return Err(AppError::localized(
            "codex_aggregate.routes_empty",
            "Codex 聚合供应商的路由表至少需要一个可用的模型槽位",
            "Codex aggregate provider requires at least one usable model route slot",
        ));
    };
    let default_model = default_entry["model"]
        .as_str()
        .ok_or_else(|| {
            AppError::localized(
                "codex_aggregate.routes_empty",
                "Codex 聚合供应商的路由表至少需要一个可用的模型槽位",
                "Codex aggregate provider requires at least one usable model route slot",
            )
        })?
        .to_string();

    let config = format!(
        "model = \"{default_model}\"\n\
         model_provider = \"{CODEX_AGGREGATE_PROVIDER_ID}\"\n\
         disable_response_storage = true\n\
         \n\
         [model_providers.{CODEX_AGGREGATE_PROVIDER_ID}]\n\
         name = \"{CODEX_AGGREGATE_PROVIDER_NAME}\"\n\
         requires_openai_auth = true\n\
         base_url = \"{proxy_origin}\"\n\
         wire_api = \"responses\"\n"
    );

    Ok(serde_json::json!({
        "auth": { "OPENAI_API_KEY": CODEX_AGGREGATE_AUTH_PLACEHOLDER },
        "config": config,
        "modelCatalog": { "models": catalog },
    }))
}

/// 该供应商是否为聚合供应商。
pub fn is_aggregate_provider(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .is_some()
}

/// 取聚合供应商的路由表；profile 派生与运行时路由解析共用同一报错。
fn routes_of(provider: &Provider) -> Result<&AggregateRoutes, AppError> {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "aggregate.routes_missing",
                "聚合供应商缺少路由表",
                "Aggregate provider is missing its route table",
            )
        })
}

/// 该供应商是否为 Codex 聚合供应商。
pub fn is_codex_aggregate_provider(provider: &Provider) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.codex_aggregate_routes.as_ref())
        .is_some()
}

/// 取 Codex 聚合供应商的路由表。与 `routes_of` 同构但查的是独立键——
/// 两套键互不相通，Codex 侧不得回落到 Claude 侧的 `aggregateRoutes`。
pub(crate) fn codex_routes_of(provider: &Provider) -> Result<&CodexAggregateRoutes, AppError> {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.codex_aggregate_routes.as_ref())
        .ok_or_else(|| {
            AppError::localized(
                "codex_aggregate.routes_missing",
                "Codex 聚合供应商缺少路由表",
                "Codex aggregate provider is missing its route table",
            )
        })
}

/// 由槽位派生模型规格（供 profile 的 inferenceModels 与 /models 端点共用）。
/// 槽位 ID 由前端在编辑时生成并持久化，这里直接取用；按 route_id 排序与既有实现保持一致。
pub fn aggregate_model_routes(provider: &Provider) -> Result<Vec<ResolvedModelRoute>, AppError> {
    let routes = routes_of(provider)?;

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
            max_effort: slot
                .max_effort
                .as_deref()
                .filter(|v| AGGREGATE_MAX_EFFORTS.contains(v))
                .map(str::to_string),
            // 枚举带 serde(rename_all="lowercase")，序列化结果即 profile/前端用的
            // 小写档位名（fable/opus/sonnet/haiku）
            tier: serde_json::to_value(slot.tier)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string)),
        });
    }
    // 保持 slots 的既有顺序（UI 提交时已按「供应商分组 × 档位强弱」展平），让
    // Claude Desktop 的选择器里同一家供应商的模型聚在一起、内部按 fable→opus→
    // sonnet→haiku 排列。**不要**按 route_id 字典序重排——那会把不同供应商的
    // 模型交错穿插（用户 2026-09-24 反馈）。去重仍按 route_id（重复只保留首次）。
    out.dedup_by(|a, b| a.route_id == b.route_id);
    // 默认模型置顶：Claude Desktop 认 inferenceModels 第一条为默认模型。引用
    // 悬空（槽已删/ID 改名）时静默忽略、保持原序——只影响启动默认，不影响路由。
    if let Some(default_model) = routes.default_model.as_deref().map(str::trim) {
        if !default_model.is_empty() {
            if let Some(pos) = out.iter().position(|route| route.route_id == default_model) {
                let pinned = out.remove(pos);
                out.insert(0, pinned);
            }
        }
    }

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
///
/// 查找前先剥离 `[1m]` 标记：槽位开了 `supports1m` 后，Claude Desktop 会给模型名
/// 加上这个后缀再发请求。不剥离的话 1M 变体会整体漏到默认目标，并被目标供应商
/// 自己的路由表改写模型名——「选 A 家的 1M 变体，实际打到 B 家的模型」。
pub fn resolve_target(
    db: &crate::database::Database,
    app_type: &str,
    aggregate: &Provider,
    request_model: &str,
) -> Result<(Provider, Option<String>), AppError> {
    let routes = routes_of(aggregate)?;

    let requested =
        crate::claude_desktop_config::strip_one_m_suffix_for_route_lookup(request_model);

    // 直接按持久化的槽位 ID 查表（ID 由前端编辑时生成并随表单提交，运行时不重新生成）
    for slot in &routes.slots {
        if slot.route_id == requested {
            let target = load_provider(db, app_type, &slot.provider_id)?;
            return Ok((target, Some(slot.upstream_model.clone())));
        }
    }

    // 别名层：槽位未命中时按序前缀匹配（小写化），命中且目标槽存在 → 视同命中该槽。
    // 悬空/空前缀静默跳过，不报错——别名是可选优化项，不是安全网。
    // 悬空（slot_id 非空但解析不到槽）记 info：这是用户在日志里唯一的自助线索
    // （症状是别名请求落到兜底）。半成品规则（slot_id 为空，用户正在编辑）不记，
    // 否则编辑期间的每次请求都会刷一行。
    let lowered = requested.to_lowercase();
    for rule in &routes.alias_rules {
        let prefix = rule.prefix.trim().to_lowercase();
        if prefix.is_empty() || !lowered.starts_with(&prefix) {
            continue;
        }
        let Some(slot) = routes.slots.iter().find(|s| s.route_id == rule.slot_id) else {
            if !rule.slot_id.is_empty() {
                log::info!(
                    "[aggregate] alias rule '{prefix}' -> dangling slot '{}', skipped",
                    rule.slot_id
                );
            }
            continue;
        };
        let target = load_provider(db, app_type, &slot.provider_id)?;
        return Ok((target, Some(slot.upstream_model.clone())));
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

/// 解析 Codex 聚合路由：给定聚合供应商与请求里的模型名，返回 (目标供应商, 需改写的
/// 上游模型名)。与 `resolve_target` 同构但查表键不同：
/// - 命中槽位 → 返回该槽位的目标与上游模型
/// - 未命中 → 返回默认目标（`ProviderId` 不改写模型名；`SlotId` 保留该槽上游模型名）
/// - 默认目标不可用 / 目标供应商缺失 → 明确错误（不静默降级）
///
/// 查找前同样先剥离 `[1m]` 标记（与 Claude 侧同一个 helper）：客户端可能给模型名
/// 加上这个后缀再发请求，不剥离的话整批变体会漏到默认目标。
///
/// Codex 侧**没有别名层**：客户端模型名是 CLI 从 modelCatalog 里选的 slug，
/// 精确匹配即可；加前缀回落只会把请求误发给用户没选的供应商。
pub fn resolve_codex_target(
    db: &crate::database::Database,
    app_type: &str,
    aggregate: &Provider,
    request_model: &str,
) -> Result<(Provider, Option<String>), AppError> {
    let routes = codex_routes_of(aggregate)?;

    let requested =
        crate::claude_desktop_config::strip_one_m_suffix_for_route_lookup(request_model);

    for slot in &routes.slots {
        if slot.model == requested {
            let target = load_codex_provider(db, app_type, &slot.provider_id)?;
            return Ok((target, Some(slot.upstream_model.clone())));
        }
    }

    // 未命中 → 默认目标
    let (fallback_id, fallback_upstream) = match &routes.default_target {
        DefaultTarget::ProviderId(id) => (id.clone(), None),
        // SlotId 兜底本身就是一份路由：保留该槽的上游模型名（与 Claude 侧同款）
        DefaultTarget::SlotId(slot_id) => {
            let slot = routes
                .slots
                .iter()
                .find(|slot| &slot.model == slot_id)
                .ok_or_else(|| {
                    AppError::localized(
                        "codex_aggregate.default_target_slot_missing",
                        "Codex 聚合供应商的默认目标指向了不存在的槽位",
                        "Codex aggregate default target points to a missing slot",
                    )
                })?;
            (slot.provider_id.clone(), Some(slot.upstream_model.clone()))
        }
    };
    let target = load_codex_provider(db, app_type, &fallback_id)?;
    Ok((target, fallback_upstream))
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

/// `load_provider` 的 Codex 版：报错带 Codex 聚合自己的 key，便于用户把日志里的
/// 问题定位到 Codex 侧路由表（两套路由表互不相通）。
fn load_codex_provider(
    db: &crate::database::Database,
    app_type: &str,
    provider_id: &str,
) -> Result<Provider, AppError> {
    db.get_provider_by_id(provider_id, app_type)?
        .ok_or_else(|| {
            AppError::localized(
                "codex_aggregate.target_provider_missing",
                "Codex 聚合供应商的目标供应商不存在",
                "Codex aggregate target provider does not exist",
            )
        })
}
