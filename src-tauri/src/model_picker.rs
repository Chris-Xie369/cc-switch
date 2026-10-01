//! Claude Code CLI `/model` 选择器的 `modelPicker` 同步（B：CLI 侧聚合）。
//!
//! 由聚合槽位路由表生成 `~/.claude/settings.json` 顶层 `modelPicker`，
//! 与 Claude Desktop profile 的 `inferenceModels` 同寿命（apply 时同步）。
//! 设计：docs/superpowers/specs/2026-10-01-cli-model-picker-design.md

use std::path::Path;

use serde_json::{json, Value};

use crate::claude_desktop_config::ResolvedModelRoute;
use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
use crate::database::Database;
use crate::error::AppError;

/// 存证最近一次写入的 modelPicker 生成值。非聚合应用删键前与它比对，
/// 不一致（用户手写/改过）则不动——settings.json 是用户领地，不误删。
const GENERATED_SNAPSHOT_KEY: &str = "cli_model_picker_generated";

/// 档位 → 本版 CLI 认识的模型 ID（behavesAs）。取证见设计 D4。
const BEHAVES_AS: [(&str, &str); 4] = [
    ("fable", "claude-fable-5"),
    ("opus", "claude-opus-4-8"),
    ("sonnet", "claude-sonnet-4-6"),
    ("haiku", "claude-haiku-4-5"),
];

fn behaves_as_for(tier: Option<&str>) -> Option<&'static str> {
    tier.and_then(|t| {
        BEHAVES_AS
            .iter()
            .find(|(key, _)| *key == t)
            .map(|(_, id)| *id)
    })
}

fn make_row(model: &str, label: Option<&str>, behaves: Option<&str>) -> Value {
    let mut row = json!({ "model": model });
    if let Some(label) = label {
        row["label"] = Value::String(label.to_string());
    }
    if let Some(behaves) = behaves {
        row["behavesAs"] = Value::String(behaves.to_string());
    }
    row
}

/// 由路由表生成 modelPicker 值。顺序 = 路由序（供应商分组 × 档位、默认模型置顶），
/// supports_1m 的槽位在本体行后追加 `[1m]` 行。
pub fn model_picker_value(routes: &[ResolvedModelRoute]) -> Value {
    let mut options: Vec<Value> = Vec::with_capacity(routes.len() * 2);
    for route in routes {
        let behaves = behaves_as_for(route.tier.as_deref());
        let label = route.label_override.clone();
        options.push(make_row(&route.route_id, label.as_deref(), behaves));
        if route.supports_1m {
            let label_1m = label.as_ref().map(|l| format!("{l} · 1M"));
            options.push(make_row(
                &format!("{}[1m]", route.route_id),
                label_1m.as_deref(),
                behaves,
            ));
        }
    }
    json!({ "replaceBuiltInOptions": false, "options": options })
}

pub(crate) fn sync_cli_model_picker(
    db: &Database,
    routes: Option<&[ResolvedModelRoute]>,
) -> Result<(), AppError> {
    sync_cli_model_picker_at(db, &get_claude_settings_path(), routes)
}

/// `Some(routes)`：写入生成值并存证。`None`：仅当文件当前值与存证一致才删键。
pub(crate) fn sync_cli_model_picker_at(
    db: &Database,
    path: &Path,
    routes: Option<&[ResolvedModelRoute]>,
) -> Result<(), AppError> {
    match routes {
        Some(routes) => {
            let value = model_picker_value(routes);
            set_top_level_key(path, "modelPicker", Some(&value))?;
            db.set_setting(GENERATED_SNAPSHOT_KEY, &value.to_string())
        }
        None => {
            let stored = db
                .get_setting(GENERATED_SNAPSHOT_KEY)?
                .filter(|s| !s.is_empty())
                .and_then(|s| serde_json::from_str::<Value>(&s).ok());
            if let Some(ours) = stored {
                let current = read_json_or_empty(path);
                if current.get("modelPicker") == Some(&ours) {
                    set_top_level_key(path, "modelPicker", None)?;
                }
                db.set_setting(GENERATED_SNAPSHOT_KEY, "")?;
            }
            Ok(())
        }
    }
}

fn read_json_or_empty(path: &Path) -> Value {
    if !path.exists() {
        return json!({});
    }
    read_json_file::<Value>(path).unwrap_or_else(|_| json!({}))
}

fn set_top_level_key(path: &Path, key: &str, value: Option<&Value>) -> Result<(), AppError> {
    if !path.exists() && value.is_none() {
        return Ok(());
    }
    let mut doc = read_json_or_empty(path);
    if !doc.is_object() {
        if value.is_none() {
            return Ok(());
        }
        if !doc.is_null() {
            return Err(AppError::Config(format!(
                "{} 不是 JSON 对象，拒绝写入 {key}",
                path.display()
            )));
        }
        doc = json!({});
    }
    match value {
        Some(v) => doc[key] = v.clone(),
        None => {
            if let Some(obj) = doc.as_object_mut() {
                obj.remove(key);
            }
        }
    }
    write_json_file(path, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_desktop_config::ResolvedModelRoute;
    use serde_json::json;

    fn route(
        route_id: &str,
        tier: &str,
        label: Option<&str>,
        supports_1m: bool,
    ) -> ResolvedModelRoute {
        ResolvedModelRoute {
            route_id: route_id.to_string(),
            upstream_model: "glm-5.3".to_string(),
            label_override: label.map(str::to_string),
            supports_1m,
            max_effort: None,
            tier: Some(tier.to_string()),
        }
    }

    #[test]
    fn rows_carry_model_label_and_behaves_as() {
        let value = model_picker_value(&[route(
            "claude-fable-1",
            "fable",
            Some("Zhipu GLM · glm-5.3"),
            false,
        )]);
        assert_eq!(value["replaceBuiltInOptions"], json!(false));
        assert_eq!(
            value["options"],
            json!([{
                "model": "claude-fable-1",
                "label": "Zhipu GLM · glm-5.3",
                "behavesAs": "claude-fable-5"
            }])
        );
    }

    #[test]
    fn appends_1m_row_only_when_supported() {
        let with = model_picker_value(&[route("claude-fable-1", "fable", Some("A"), true)]);
        assert_eq!(with["options"].as_array().unwrap().len(), 2);
        assert_eq!(with["options"][1]["model"], json!("claude-fable-1[1m]"));
        assert_eq!(with["options"][1]["label"], json!("A · 1M"));
        let without = model_picker_value(&[route("claude-fable-1", "fable", Some("A"), false)]);
        assert_eq!(without["options"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn omits_label_when_missing() {
        let value = model_picker_value(&[route("claude-fable-1", "fable", None, true)]);
        assert!(value["options"][0].get("label").is_none());
        // 无基名 → 1M 行同样省略（默认渲染 model 名，自带 [1m] 可区分）
        assert!(value["options"][1].get("label").is_none());
    }

    #[test]
    fn maps_each_tier_to_its_known_id() {
        for (tier, known) in [
            ("fable", "claude-fable-5"),
            ("opus", "claude-opus-4-8"),
            ("sonnet", "claude-sonnet-4-6"),
            ("haiku", "claude-haiku-4-5"),
        ] {
            let value = model_picker_value(&[route("claude-x-1", tier, None, false)]);
            assert_eq!(
                value["options"][0]["behavesAs"],
                json!(known),
                "tier {tier}"
            );
        }
    }

    #[test]
    fn preserves_route_order() {
        let value = model_picker_value(&[
            route("claude-opus-4-8", "opus", Some("A"), false),
            route("claude-fable-1", "fable", Some("B"), true),
            route("claude-sonnet-4", "sonnet", Some("C"), false),
        ]);
        let models: Vec<_> = value["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["model"].as_str().unwrap())
            .collect();
        assert_eq!(
            models,
            [
                "claude-opus-4-8",
                "claude-fable-1",
                "claude-fable-1[1m]",
                "claude-sonnet-4"
            ]
        );
    }

    #[test]
    fn sync_writes_key_and_preserves_neighbors() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"theme":"dark","env":{"A":"b"}}"#).unwrap();
        sync_cli_model_picker_at(
            &db,
            &path,
            Some(&[route("claude-fable-1", "fable", Some("X"), false)]),
        )
        .unwrap();
        let saved: Value = read_json_file(&path).unwrap();
        assert_eq!(saved["theme"], json!("dark"));
        assert_eq!(saved["env"], json!({"A":"b"}));
        assert_eq!(
            saved["modelPicker"]["options"][0]["model"],
            json!("claude-fable-1")
        );
    }

    #[test]
    fn sync_removes_key_when_snapshot_matches() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let routes = [route("claude-fable-1", "fable", Some("X"), false)];
        sync_cli_model_picker_at(&db, &path, Some(&routes)).unwrap();
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        let saved: Value = read_json_file(&path).unwrap();
        assert!(saved.get("modelPicker").is_none());
    }

    #[test]
    fn sync_keeps_handwritten_picker() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"modelPicker":{"options":[{"model":"claude-opus-5"}]}}"#,
        )
        .unwrap();
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        let saved: Value = read_json_file(&path).unwrap();
        assert_eq!(
            saved["modelPicker"]["options"][0]["model"],
            json!("claude-opus-5")
        );
    }

    #[test]
    fn sync_reapply_overwrites_previous_picker() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        sync_cli_model_picker_at(
            &db,
            &path,
            Some(&[route("claude-fable-1", "fable", Some("X"), false)]),
        )
        .unwrap();
        sync_cli_model_picker_at(
            &db,
            &path,
            Some(&[route("claude-opus-4-8", "opus", Some("Y"), false)]),
        )
        .unwrap();
        let saved: Value = read_json_file(&path).unwrap();
        let models: Vec<_> = saved["modelPicker"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["model"].as_str().unwrap())
            .collect();
        assert_eq!(models, ["claude-opus-4-8"]);
    }
}
