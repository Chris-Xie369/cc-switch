# 独立代码审查报告 — 聚合路由身份与存量无损编辑（task_cfc43aae54a58adb）

审查对象不可变 SHA256：3f02a6bbcc96aeaef56d8be76cb09019efbeff957335ba7b6a89f5af7cfa239f。本审查仅基于冻结包内源码、diff 与文本证据独立推理，未执行代码、未访问环境；模型/供应商/档位/强度仅视为宿主声明而非供应商身份证明，浏览器截图由控制器目检，此处不做像素级验收。

## 总体结论

passed（critical 0 / major 0 / minor 7）。实现与批准的新契约一致，未发现阻塞性缺陷、回归或意外数据丢失。已交付的账号修复保持完好：diff 未触碰 proxy/forwarder 与认证绑定路径，证据日志中 aggregate_auth_regression_tests 9 项全部通过。

## 逐项核对（对照审查范围）

1. 稳定已有 ID：新版 assignSlotIds 对非空 routeId 原样保留（仅浅拷贝），只为 routeId 为空的新行分配 ID。重排（reverse）、插入新行、编辑档位元数据/上游/显示名的往返测试与组件测试均验证身份、defaultModel、defaultTarget、aliasRules 不变。池内固化 ID（sonnet-3/fable-6）与溢出跳名逻辑经单测覆盖且与注释一致。
2. 增/删/重排/默认/别名：删除的槽位 ID 记入 retired（含从 previous 与本表双方合并）；默认模型悬空置空回落排序首位；默认目标悬空保持原值、加入 taken 防新行认领，由 canSaveAggregateRoutes 新增的槽位存在性检查与后端保存校验双重拦截；别名悬空清空保行、空 slotId 不被新行认领。删除兜底槽位后旧名不会被静默改指。
3. 退役 ID 持久化与先拒：resolve_target 在槽位精确匹配、别名、兜底之前对退役名（trim + eq_ignore_ascii_case）硬报错；Rust 测试覆盖裸名、[1m] 后缀变体、全大写变体；serde default/skip_serializing_if 与前端可选字段省略对齐，旧数据（无该键）反序列化为空且空数组不序列化（往返测试验证）。
4. 同供应商同档多行：rows 由单槽改为数组，groupSlotsByProvider/flattenProviderGroups 保序往返；组件测试验证双行同时显示、编辑其一保留其余及身份、删除其一后另一行可用、同档可继续新增。
5. 显式预览与备份的迁移：needsRouteIdMigration 仅标记不兼容/重复 ID（普通编辑绝不自动转换，测试锁定）；migrateIncompatibleRouteIds 重映射 defaultTarget/defaultModel/aliasRules，按新 ID 实际阶梯剥离不支持的 maxEffort，旧 ID 记退役；UI 强制先下载备份（migrationBackup 门禁）后确认；useEffect([value]) 使任何值变更后的预览与确认失效（测试锁定）。重复 ID 迁移后首个同 ID 槽保持活跃、不被误记退役。
6. 新旧序列化与编译完整性：Rust 侧四处既有结构体字面量（aggregate.rs、claude_desktop_config.rs、proxy/server.rs、stream_check.rs、services/provider/mod.rs）全部补齐新字段；cargo test 102 项通过佐证无遗漏构造点。保存校验新增退役 ID 非空且不与活跃 ID 重叠（大小写不敏感），测试覆盖三种非法形态。
7. 测试/证据声明核查：前端日志 77 项 = 6+36+2+3+4+6+20，与冻结测试文件逐描述块清点完全一致（aggregateRoutes.test.ts 恰 36 项），证明冻结源码与实际执行的测试同源；后端 102 项含 4 项退役测试与新保存校验测试；文档明确不宣称约 3000 项后端全套通过（2935 filtered 与日志一致）、明确浏览器未执行 Tauri 保存与迁移确认分支（由组件测试覆盖）、明确旧版本回退会丢弃退役历史并重编号——证据表述与代码行为一致，无夸大或虚构执行。
8. 回归/数据丢失排查：group→flatten 会按卡序×档位固定序重排持久化槽位顺序（仅显示顺序变化，模型零丢失、身份不变，属文档声明行为）；commit 对所有槽按其最终 ID 归一化 maxEffort，对无效残留是清理而非丢失；迁移确认绕过 commit 的二次分配属有意设计（避免重复退役）。未发现其他静默改指或丢数据路径。

## 发现（全部 minor）

- F1 迁移预览仅逐槽显示 ID 与强度上限变化，未展示 defaultTarget/defaultModel/aliasRules 的重映射结果；映射经测试验证无损且有备份兜底，但确认前透明度不足（AggregateProviderFields.tsx 预览 pre 块）。
- F2 needsRouteIdMigration/migrate 的重复判定大小写敏感（seen 未归一化），与退役检查/新 ID 分配/保存重叠校验的大小写不敏感不对称：大小写变体重复 ID 不触发迁移提示、可通过保存（后端重复校验同为大小写敏感，系统内自洽），但退役层运行时是宽匹配（aggregateRoutes.ts）。
- F3 迁移横幅硬编码中文，绕过组件其余部分的 t()+defaultValue 模式，非中文环境不翻译（AggregateProviderFields.tsx）。
- F4 迁移确认不经 commit，ID 未变槽位的越阶 maxEffort（如 4-6 + xhigh，后端白名单放行）随迁移保留至下一次普通编辑才剥离，期间 Desktop 静默压档（aggregateRoutes.ts migrate 分支）。
- F5 retiredRouteIds 只增不减、无查看/清理入口，被删 ID 永久不可有意复用；属批准契约取舍，缺可观测性（types.ts / aggregate.rs）。
- F6 文档声称的 tsc --noEmit 与 cargo check 通过在冻结包内无日志佐证（vitest/cargo test 日志齐备且与源码清点一致，属证据链小缺口）（docs/reviews/2026-10-04-aggregate-route-compatibility-fix.md）。
- F7 MaxEffortSelect 禁用态 tooltip 对 extended ID 误用 none 态文案；既有问题，非本次引入（AggregateProviderFields.tsx）。

## 审查边界

后端 is_claude_safe_model_id 具体实现不在包内，前端 isCompatibleRouteId 为注释声明对齐的镜像（保存校验以后端为权威，存在双份维护的漂移风险，本次不计为缺陷）；strip_one_m_suffix_for_route_lookup 实现未提供，假定其在退役比较前执行（退役测试覆盖 [1m] 变体佐证）。以上均不构成阻塞。

结论：passed。实现质量高，身份稳定性、退役先拒、显式迁移门禁与序列化兼容均经代码推理与测试清点独立证实；7 项 minor 均为透明度、一致性或可观测性改进项，不阻塞交付。

控制者证据补充：SDK exit0，原响应的一个多余闭合括号保留，解析未改裁决字段。审查只读取一次输入中的源码/文本，不冒充执行工具或视觉验收。F6所述日志在完整冻结ZIP中存在：evidence/route-final-production-check.txt及route-typecheck-observation.json；初次模型输入未内嵌这两项，限制保留。
