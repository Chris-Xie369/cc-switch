use super::*;

fn routes_json() -> serde_json::Value {
    serde_json::json!({
        "slots":[{"routeId":"claude-sonnet-4-5","tier":"sonnet","providerId":"synthetic-target","upstreamModel":"new-model"}],
        "defaultTarget":{"kind":"providerId","value":"synthetic-target"},
        "aliasRules":[{"prefix":"claude-sonnet","slotId":"claude-sonnet-4-5"}],
        "retiredRouteIds":["claude-sonnet-4-6"]
    })
}

#[test]
fn aggregate_retirement_survives_backend_serialization_roundtrip() {
    let routes: AggregateRoutes = serde_json::from_value(routes_json()).unwrap();
    let persisted = serde_json::to_value(routes).unwrap();
    assert_eq!(
        persisted["retiredRouteIds"],
        serde_json::json!(["claude-sonnet-4-6"])
    );
}

#[tokio::test]
async fn aggregate_retirement_old_requests_are_rejected_before_alias_or_fallback() {
    let db = crate::database::Database::memory().unwrap();
    let target = Provider::with_id(
        "synthetic-target".into(),
        "Synthetic".into(),
        serde_json::json!({}),
        None,
    );
    db.save_provider("claude-desktop", &target).unwrap();
    let mut provider = Provider::with_id(
        "synthetic-aggregate".into(),
        "Synthetic Aggregate".into(),
        serde_json::json!({}),
        None,
    );
    provider.meta = Some(crate::provider::ProviderMeta {
        aggregate_routes: Some(serde_json::from_value(routes_json()).unwrap()),
        ..Default::default()
    });
    for request in [
        "claude-sonnet-4-6",
        "claude-sonnet-4-6[1m]",
        "CLAUDE-SONNET-4-6",
    ] {
        assert!(
            resolve_target(&db, "claude-desktop", &provider, request).is_err(),
            "Retired ID must not be redirected through the alias or default"
        );
    }
}

#[test]
fn aggregate_retirement_legacy_absent_metadata_is_backward_compatible() {
    let mut legacy = routes_json();
    legacy.as_object_mut().unwrap().remove("retiredRouteIds");
    let parsed: AggregateRoutes = serde_json::from_value(legacy).unwrap();
    let persisted = serde_json::to_value(parsed).unwrap();
    assert!(persisted.get("retiredRouteIds").is_none());
}

#[tokio::test]
async fn aggregate_compatibility_padded_route_key_preserves_exact_mapping() {
    let db = crate::database::Database::memory().unwrap();
    db.save_provider(
        "claude-desktop",
        &Provider::with_id(
            "synthetic-target".into(),
            "Synthetic".into(),
            serde_json::json!({}),
            None,
        ),
    )
    .unwrap();
    let mut json = routes_json();
    json["slots"][0]["routeId"] = serde_json::json!(" claude-sonnet-4-5 ");
    json["aliasRules"] = serde_json::json!([]);
    let mut provider = Provider::with_id(
        "synthetic-aggregate".into(),
        "Synthetic".into(),
        serde_json::json!({}),
        None,
    );
    provider.meta = Some(crate::provider::ProviderMeta {
        aggregate_routes: Some(serde_json::from_value(json).unwrap()),
        ..Default::default()
    });
    let (_, upstream) =
        resolve_target(&db, "claude-desktop", &provider, "claude-sonnet-4-5").unwrap();
    assert_eq!(upstream.as_deref(), Some("new-model"));
}
