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

// 测试探针：宽松读（`read_json_or_empty`）的调用次数。删路径只允许读一次
// （比对与写回共用同一份文档），计数器让「二次读」回归可被测试捕获。
#[cfg(test)]
thread_local! {
    static LENIENT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
        // 空白 label 视为无 label（D4）：否则会写出空标签行与裸 " · 1M" 后缀行
        let label = route
            .label_override
            .as_deref()
            .filter(|l| !l.trim().is_empty());
        options.push(make_row(&route.route_id, label, behaves));
        if route.supports_1m {
            let label_1m = label.map(|l| format!("{l} · 1M"));
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
            set_top_level_key(path, "modelPicker", &value)?;
            db.set_setting(GENERATED_SNAPSHOT_KEY, &value.to_string())
        }
        None => {
            let stored = db
                .get_setting(GENERATED_SNAPSHOT_KEY)?
                .filter(|s| !s.is_empty());
            let ours = stored
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            if let Some(ours) = ours {
                let current = read_json_or_empty(path);
                if current.get("modelPicker") == Some(&ours) {
                    // 只写回已比对过的那份文档：再读一次若恰逢文件被改坏/锁定，
                    // 宽松读会退化成 {} 并把整份用户配置覆盖成空（D5）。
                    let mut doc = current;
                    if let Some(obj) = doc.as_object_mut() {
                        obj.remove("modelPicker");
                        write_json_file(path, &doc)?;
                    }
                }
            }
            // 存证无论能否解析都要清空：留着的陈旧存证会让下次删键比对失准
            if stored.is_some() {
                db.set_setting(GENERATED_SNAPSHOT_KEY, "")?;
            }
            Ok(())
        }
    }
}

/// 删路径专用宽松读：读失败按空对象处理（D5，结果必然是 no-op，保守无害）。
/// 只许删路径调一次——比对与写回共用这一份文档，杜绝「读两次」的 TOCTOU 覆盖。
fn read_json_or_empty(path: &Path) -> Value {
    #[cfg(test)]
    LENIENT_READS.with(|n| n.set(n.get() + 1));
    if !path.exists() {
        return json!({});
    }
    read_json_file::<Value>(path).unwrap_or_else(|_| json!({}))
}

/// 写路径严格读：读失败（parse/IO）上抛，绝不整份覆盖用户配置（D5）；
/// 只有文件不存在才从空对象起步。
fn read_json_strict(path: &Path) -> Result<Value, AppError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    read_json_file::<Value>(path)
}

fn set_top_level_key(path: &Path, key: &str, value: &Value) -> Result<(), AppError> {
    let mut doc = read_json_strict(path)?;
    if !doc.is_object() {
        if !doc.is_null() {
            return Err(AppError::Config(format!(
                "{} 不是 JSON 对象，拒绝写入 {key}",
                path.display()
            )));
        }
        doc = json!({});
    }
    doc[key] = value.clone();
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
        assert_eq!(with["options"][1]["behavesAs"], json!("claude-fable-5"));
        assert_eq!(
            with["options"][1]["behavesAs"],
            with["options"][0]["behavesAs"]
        );
        let without = model_picker_value(&[route("claude-fable-1", "fable", Some("A"), false)]);
        assert_eq!(without["options"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn omits_label_when_missing() {
        let value = model_picker_value(&[route("claude-fable-1", "fable", None, true)]);
        let options = value["options"].as_array().unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(options[0]["model"], json!("claude-fable-1"));
        assert_eq!(options[1]["model"], json!("claude-fable-1[1m]"));
        assert!(options[0].get("label").is_none());
        // 无基名 → 1M 行同样省略（默认渲染 model 名，自带 [1m] 可区分）
        assert!(options[1].get("label").is_none());
    }

    /// D4「空则省略」：空白 label 等价于无 label，否则 1M 行会写出裸 " · 1M"
    #[test]
    fn omits_label_when_blank() {
        for blank in ["", "  "] {
            let value = model_picker_value(&[route("claude-fable-1", "fable", Some(blank), true)]);
            let options = value["options"].as_array().unwrap();
            assert_eq!(options.len(), 2, "blank {blank:?}");
            assert!(options[0].get("label").is_none(), "blank {blank:?}");
            assert!(options[1].get("label").is_none(), "blank {blank:?}");
        }
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
        std::fs::write(&path, r#"{"theme":"dark","env":{"A":"b"}}"#).unwrap();
        let routes = [route("claude-fable-1", "fable", Some("X"), false)];
        sync_cli_model_picker_at(&db, &path, Some(&routes)).unwrap();
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        let saved: Value = read_json_file(&path).unwrap();
        assert!(saved.get("modelPicker").is_none());
        assert_eq!(saved["theme"], json!("dark"));
        assert_eq!(saved["env"], json!({"A":"b"}));
        assert_eq!(
            db.get_setting(GENERATED_SNAPSHOT_KEY).unwrap().as_deref(),
            Some("")
        );
    }

    /// 存证一致但文件值已被手改 → 不得删（D3 最易错的判别分支）
    #[test]
    fn sync_keeps_edited_picker_when_snapshot_differs() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let routes = [route("claude-fable-1", "fable", Some("X"), false)];
        sync_cli_model_picker_at(&db, &path, Some(&routes)).unwrap();
        std::fs::write(
            &path,
            r#"{"theme":"dark","modelPicker":{"options":[{"model":"claude-opus-5"}]}}"#,
        )
        .unwrap();
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        let saved: Value = read_json_file(&path).unwrap();
        assert_eq!(
            saved["modelPicker"]["options"][0]["model"],
            json!("claude-opus-5")
        );
        assert_eq!(saved["theme"], json!("dark"));
        assert_eq!(
            db.get_setting(GENERATED_SNAPSHOT_KEY).unwrap().as_deref(),
            Some("")
        );
    }

    /// 存证是垃圾字符串（非空但解析失败）→ 不删键，但存证必须清掉
    #[test]
    fn sync_clears_corrupt_snapshot_without_touching_file() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let original = r#"{"modelPicker":{"options":[{"model":"claude-opus-5"}]},"theme":"dark"}"#;
        std::fs::write(&path, original).unwrap();
        db.set_setting(GENERATED_SNAPSHOT_KEY, "{not json").unwrap();
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert_eq!(
            db.get_setting(GENERATED_SNAPSHOT_KEY).unwrap().as_deref(),
            Some("")
        );
    }

    /// D5 删路径宽松：读失败按空对象处理，结果必然是 no-op 且不动文件
    #[test]
    fn sync_remove_is_noop_when_read_fails() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let broken = r#"{"theme":"dark","modelPicker":{"options":[{"model":"x"}"}"#;
        std::fs::write(&path, broken).unwrap();
        db.set_setting(GENERATED_SNAPSHOT_KEY, &json!({"options":[]}).to_string())
            .unwrap();
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        assert_eq!(
            db.get_setting(GENERATED_SNAPSHOT_KEY).unwrap().as_deref(),
            Some("")
        );
    }

    #[test]
    fn sync_write_fails_and_preserves_file_when_read_fails() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let broken = r#"{"theme":"dark","modelPicker":{"options":[{"model":"x"}"}"#;
        std::fs::write(&path, broken).unwrap();
        let result = sync_cli_model_picker_at(
            &db,
            &path,
            Some(&[route("claude-fable-1", "fable", Some("X"), false)]),
        );
        assert!(result.is_err(), "读失败必须上抛而不是覆盖写");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
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

    /// D3/D5 删路径单读：比对与写回共用同一份文档。
    ///
    /// 这不是行为断言而是结构性断言——两次读之间夹着 IO，测试无法确定性地
    /// 把文件改坏/上锁来模拟竞态，因此改为直接数宽松读次数（唯一的文件读入口
    /// 就是 `read_json_or_empty`）。任何人在删路径重新引入一次读，计数即 >1。
    #[test]
    fn sync_delete_uses_already_read_document_without_rereading() {
        let db = Database::memory().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"theme":"dark","env":{"A":"b"}}"#).unwrap();
        let routes = [route("claude-fable-1", "fable", Some("X"), false)];
        sync_cli_model_picker_at(&db, &path, Some(&routes)).unwrap();

        LENIENT_READS.with(|n| n.set(0));
        sync_cli_model_picker_at(&db, &path, None).unwrap();
        let reads = LENIENT_READS.with(|n| n.get());
        assert_eq!(reads, 1, "删路径只能宽松读一次（比对即写回的那一份）");

        // 写回的是比对过的那份文档：只有 modelPicker 被摘掉，邻键完好
        let saved: Value = read_json_file(&path).unwrap();
        assert!(saved.get("modelPicker").is_none());
        assert_eq!(saved["theme"], json!("dark"));
        assert_eq!(saved["env"], json!({"A":"b"}));
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
