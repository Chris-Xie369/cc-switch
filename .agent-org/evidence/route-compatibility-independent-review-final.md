# 独立复审报告 — 聚合路由身份与存量无损编辑 v2（task_cfc43aae54a58adb，会话 20261004_202748_7004d2）

新审查对象不可变 SHA256：63e2bb93efd8a52f3c481cfef139976e43231e65c82551811d1076e3de59747e。v1 裁决（C0M0Minor7）与原始响应按记录保留，但不用于通过本主体；本复审仅针对 v1→v2 实际增量与新增证据独立推理，未执行任何代码或访问环境。

## v1 发现逐项复核

- F1（预览缺默认/别名映射）→ 已修复。预览新增兜底目标、默认模型逐行 old→new 与别名规则 prefix: old→new 展示；route-v2-preview-dom.txt 的实际 DOM 快照与代码一致（claude-fable-zhipu-glm → claude-fable-1 双行均现，备份前确认禁用）。残留展示语境小疵记为新 F9（见下）。
- F2（大小写不对称）→ 已修复且双侧闭合。前端 needsRouteIdMigration/migrate 改为小写归一判定；后端保存校验重复判定改 eq_ignore_ascii_case；新增前端用例（大小写变体重复触发显式迁移并整组退役）与后端用例（validate_aggregate_rejects_case_variant_active_duplicate_keys）均提供真实 RED 日志（route-review-findings-red.txt 2 failed、route-review-case-validation-red.txt 1 FAILED）后转绿。退役检查、ID 分配 taken、保存校验、迁移判定五处现全部大小写不敏感，体系自洽。
- F3（硬编码中文）→ 已修复。横幅、三按钮、上限/不限制均改 t()+defaultValue，zh/en/ja 三语键值随包提供，zh 文案与 defaultValue 等值（DOM 中文渲染无法区分接线与否，en/ja 接线缺文件级证据，记为新 F8）。
- F4（确认绕过 commit、未变 ID 的越阶上限残留）→ 已修复。migrate 的 maxEffort 归一化改为对全部槽位按其最终 ID 实际阶梯判定，不再限定 ID 变化的槽位；新增用例「confirmed migration also normalizes an unsupported cap on an unchanged valid ID」带 RED 证据后转绿。与 commit 路径行为对齐。
- F6（tsc/cargo check 证据缺失）→ 已闭合。route-typecheck-observation.json 记录 pnpm exec tsc --noEmit exit_code 0；route-v2-production-check.txt 提供 cargo check 完成日志（注：日志为默认 dev profile，与所引命令 cargo check --offline --lib 的默认行为一致，文档「生产」措辞偏松但非虚假声明）。
- F5、F7 → 本次增量未涉及，任务方确认在实现范围外；经核仍然准确，按许可保留为 minor。

## 新语义独立核验（重复组整体退役）

migrateIncompatibleRouteIds 改为先按小写计数，重复组内全部槽位（含原本合法的首个）置空后由 assignSlotIds 重发新 ID，两变体旧名经 previous 差集进入 retired。该语义是自洽的唯一解：若保留任一活动同名路由又退役该名，将触发保存校验的 retired-active 重叠拒绝。remap 按原始字符串首现映射，defaultTarget/defaultModel/aliasRules 引用随迁；被退役名进入 taken，新 ID 分配（池内固化 + 溢出让位）不会回认领。逐一推演 v1 保留用例「explicit duplicate-ID migration…」在新逻辑下的结果与更新后的断言（4-6/4-5、上限均剥）完全一致；「allocation does not reuse a retired ID with different letter case」等既有用例不受影响。needsRouteIdMigration 的小写化对 isCompatibleRouteId 结果无副作用（其内部本就 trim+lowercase）。单一大小写变体槽（非重复）两侧均判合法、不触发迁移，与后端 claude-safe 判定及运行时大小写敏感精确匹配一致。

## 新证据一致性清点

前端 79 = 8+36+2+3+4+6+20，与兼容用例新增 2 项后的日志完全吻合；后端 103 = 102+1，新用例在列且 ok，filtered 2935 与文档一致；文档测试计数已同步更新且继续不宣称全量后端通过。账号修复完整性复核：proxy::forwarder::aggregate_auth_regression_tests 9 项在 v2 后端日志全部通过，diff 未触碰认证/转发路径。translate 键与组件 t() 调用逐一对应（migrationHint/Preview/Backup/Confirm/Limits/Unlimited + 既有 defaultTarget/defaultModel）。

## 结论

passed（C0 M0 minor 4：F5、F7 保留 + 新 F8 证据链小缺口、F9 展示语境小疵）。五项 v1 可修复发现全部经代码推理与 RED→GREEN 证据闭合，无新增阻塞性缺陷、回归或数据丢失路径；重复组整体退役语义经推演为自洽且必要。模型/供应商/档位/有效强度仍仅视为宿主声明，未做原生/像素验收，与审查边界一致。

控制者证据补充（不冒充审查者新增执行）：v2 ZIP含source与diff下zh/en/ja三文件；后续真实组件英文与日文迁移控件显示已观察、截图已检查并另行归档。其余旧日文聚合标题/提示缺译仍可见，未宣称全界面语言验收。原F8的模型输入可见性限制与原裁决保留。
