# 聚合模型评估交付：独立审查

日期：2026-10-04。任务：`task_d60aed633eec27de`。独立调度上下文 / review_run：`/root/cc_switch_aggregate_review`。登记 reviewer：`cc-switch-aggregate-independent-reviewer`，executor：`project-llm`，model/provider/effort：`gpt-6.1-sol / codex-desktop / high`。上述身份来自控制者登记与实际独立宿主调度声明；宿主 spawn 返回 canonical task_name，没有另外提供会话 UUID，也不构成供应商身份认证。

## 结论与适用范围

**review_result：passed。评估报告阻断缺陷：Critical=0，Major=0。** 独立核验支持评估报告的六项 Major 与三项 Minor 产品发现，未发现影响交付结论的误报或夸大。这里的通过只表示冻结评估交付有准确、可核验的依据与可执行改善方案；**产品仍有已记录红灯，未修复，未通过完整桌面、真实账号或上游端到端验收。** 原报告的 Critical=0 / Major=6 / Minor=3 是产品评估结果，不能作为本审查对评估报告的缺陷计数。

审查对象为冻结 artifact `.agent-org/evidence/cc-switch-aggregate-evaluation-package.zip`，SHA256：`33ede2ca15598f9cbd4170ce9a7de7e56fe9a279ee0afbefc3f4a504ae39ea4d`。其中评估报告 `report/2026-10-04-aggregate-model-evaluation.md` SHA256：`ba87f7b8530c69e235c59809ec7a57594e2e567bf736508c47b7a7af8dcd5131`。检查现有 review-package 与 status 一次，没有重准备、导入或 finish。

## 冻结基线与操作边界

- 重新计算整个 zip 的哈希，与登记 subject 一致。安全检查包内路径后解压至新建独立目录 `D:\Sources\Code\agent-org\dist\independent-cc-switch-review-20261004`；没有修改 zip。
- manifest 32 个列出的文件全部逐项哈希通过，无漏列文件，除了 manifest 自身。14 个冻结产品源码文件与当前工作目录逐项比对均一致。
- 当前 HEAD 为 `9787b2f06d9942dbc637e7b89d4aad6980cce3dd`；现有未提交 `src-tauri/src/proxy/forwarder.rs` SHA256 为 `e9b28121d474a7caf6d4ce9a3e0fa1a422e5a5889a9c0f6a85f48a2e8ea55b28`，与包内基线一致。未修改产品源码、真实配置、凭据或数据库。
- 只读冻结报告、源码、fixture、已归档脱敏日志和截图；独立运行冻结前端纯函数的小规模 fixture 检查。未启动 OAuth 测试、完整应用、上游请求、全产品测试或再次编译 Rust。
- 使用 view_image 对四张冻结截图进行视觉检查。截图只能证明真实表单组件与模拟数据的已记录行为，不能证明原生保存、实际客户端或真实账号链路。
- 原评估已经如实记录首次 OAuth 测试未完全隔离、读取本机认证内容的事件，并保留随后模拟目录串行执行的失败证据。不能把这次历史事件改写为完全隔离；本次独立审查没有读取实际认证文件或重跑其入口。

## 产品发现的独立核验

以下行号针对冻结 `source/` 内路径；14 份文件均与当前源码哈希相同。

| 发现 | 独立核验与关键路径 | 判断及边界 |
|---|---|---|
| M01 路由 ID 被显示顺序重分配 | `src/utils/aggregateRoutes.ts:192-205` 按数组和 tier 的 ordinal 重新生成 routeId；`AggregateProviderFields.tsx:173-195` 每次提交调用该逻辑，拖拽 `:283-289` 也走 commitCards；`src/types.ts:135` 称其为稳定键。独立冻结纯函数 fixture 中，旧 `claude-sonnet-4-6` 从 upstream-a 改到 upstream-b；本表默认与别名跟随到 `claude-sonnet-4-5`，无法修复外部旧会话 ID。 | 成立，既有请求身份与显示排序耦合。独立复现的是确定性表转换，无真实网络、会话或费用实测。 |
| M02 存量同供应商同档位多模型丢失 | `aggregateRoutes.ts:151-165` 每个 provider+tier 只取第一个，`:171-180` 展平无法恢复；表单 `:154` 分组和 `:195` 提交连接。独立 fixture 中两个 upstream 变一条。后端 `services/provider/mod.rs:7953-7988` 只对合法 routeId 判重，没有 provider+tier 唯一约束。 | 成立，当前后端允许这种有效形状；不能以当前 UI 不能新增它为理由静默删除。未据此声称已损坏用户真实存量。 |
| M03 Codex 官方 providerId 兜底失去聚合认证来源 | `aggregate.rs:1562-1584` providerId 返回 None；`handler_context.rs:341-372` 返回 upstream Option 并切换为目标。`forwarder.rs:1367` 在 None 时进入普通官方认证校验，`:2125-2127` 托管注入又要求 Some；seed `aggregate.rs:1335` 用 PROXY_MANAGED。`forwarder.rs:120` 会拒绝该占位符。 | 成立，请求来源被可选模型改写字段代替。slotId/精确命中不能覆盖此分支。拒绝可能早于占位符校验，例如绑定账号解析失败；结论是这条兜底不能走预期聚合托管认证，不是凭据泄露或占位符已出站。 |
| M04 显式账号绑定失败后回落另一登录文件 | `forwarder.rs:165-194` 无论有无绑定，任意 Manager 错误均调用文件回落；`:209-229` 只验证 chatgpt 模式及非空 token，未校验与绑定账号一致，workspace 允许空。`:5990-5996` 既有 dangling 测试与归档 `rust-oauth-isolated-red.txt` 的模拟 unrelated workspace 失败一致。 | 成立，源码和已归档模拟红灯相互支持。此次不重跑认证入口、不确认任何真实令牌或账号，不声称发生真实出站。P0 优先处理账号边界合理。 |
| M05 聚合单目标跳过目标熔断 | `handler_context.rs:360-366` 换目标并 truncate(1)，注释约定目标熔断显式失败；`forwarder.rs:621` 以 providers.len()==1 bypass，`:644-651` 不再获取目标许可。前置 `provider_router.rs:45-118` 操作解析前的候选供应商；故障转移关闭时本来就不检查熔断，开启时也不能替代聚合目标检查。 | 成立，与自身设计约定冲突。是源码结论，未制造真实故障或测量等待增加。保留不跨供应商故障转移与检查目标许可可以分别实现。 |
| M06 Claude slotId 兜底仅保留供应商 | `aggregate.rs:1506-1522` 只提取 provider_id 后返回 None；Codex `:1565-1580` 保留 upstream。`forwarder.rs:1425-1427` 对 Claude None 调用目标普通映射；`claude_desktop_config.rs:765-809` 可映射到目标另一档位，也可能 route_unknown。UI `AggregateProviderFields.tsx:364-367` 显示供应商和指定槽位模型。 | 成立，固定槽位与供应商规则的语义存在落差。原报告限定可能失败及触发条件，没有误称所有内部请求失败。 |
| U01 Codex 重命名/删除留下默认引用 | `CodexAggregateFields.tsx:58-66` 只改 slots；before/renamed 截图和 DOM 均显示默认控件从有值变空，但 JSON 的 defaultTarget/defaultModel 仍为 team-smart。后端 `provider/mod.rs:8071-8078` 拦悬空兜底，目录派生忽略悬空默认模型。 | 成立。属于编辑反馈与引用维护问题，报告明确后端拦截，未误称无校验的数据损坏。 |
| U02 能力提示与实际支持混淆 | `AggregateProviderFields.tsx:891` 对任何非 ladder ID 禁用上限，`:904-905` 却使用“溢出 ID，无思考控件”提示；冻结 Claude expanded 截图中第二行有思考开关。 | 成立。客户端能力由本地兼容表声明，不证明上游支持；本次没有重新检验实际 Claude Desktop 的能力表版本。 |
| U03 初次配置的信息层次与名称宽度 | 四张截图符合 1280px 隔离预览，Claude 别名在模型卡之前，显示名/部分上游名称可见截断，Codex 四列含选择/拉取操作。新增 Claude 卡的展开状态可见，源码 `AggregateProviderFields.tsx:272` 明确展开新卡。 | 作为界面观察与设计建议成立。未覆盖小窗口、缩放、键盘和原生容器，不能视为响应式失败或可访问性验收。 |

独立纯函数结果保存在 `D:\Sources\Code\agent-org\dist\independent-cc-switch-review-20261004\independent-pure-function-check.json`，SHA256 `33c03dcb2779a1acaa27c228bbc47dc9f8c6bf3b996a1d4ddf2d0a721bc11e33`。使用现有 TypeScript 编译依赖只擦除冻结模块的类型，将模块在无 require 的 VM 上下文中执行；未载入应用或调用其原生 API。这是独立行为复核，不把发现仍存在说成原兼容性要求转绿。

## 证据统计、遗漏和夸大检查

归档日志直接支持 Codex 20 passed、Rust aggregate 38 passed/2986 filtered、隔离 OAuth 3 passed/1 failed、新兼容性测试 2 failed。前三个前端文件的 46 passed 来自 test-report 中透明注明的控制者实际工具观察，未保存原始 stdout；类型检查 exit 0 也是明确标注的观察摘要。110=46+20+38+4+2，107=46+20+38+3，3=1+2，统计一致；首次未隔离重复 4 项未加总，没有掩盖产品红灯。

证据强度有两个非阻断改进空间：未来冻结包应尽量同时保存 46 项前端测试的原始退出码/stdout，以及执行起止、命令与隔离参数的脱敏记录；截图 fixture 仍引用项目 CSS/i18n 和依赖，重现完整 UI 环境需要同一 checkout 与依赖，四张图片提供的是本次已观察状态。原报告已说明这些来源与边界，不把缺失原始 stdout 伪称为原始日志，不要求为了审查重跑危险认证入口。

独立检查确认报告没有遗漏本次交付必须说明的基线、dirty 改动、失败历史和账号测试边界。审查是对列出的高价值路径和交付质量的检查，不是对所有产品路径的穷尽证明。`handler_context` 的 current_provider_id 抑制与 `forwarder.rs:721-723` 配合，原报告没有重复误报成功后切走聚合；`response_processor.rs:246-257`、`:642-648` 已有返回/出站模型计价锚点，报告把聚合费用核对保留为待验收，未把现有费用计算说成确定错误。

## 改善方案可执行性

阶段 A 先处理显式账号失效与来源字段，B 处理稳定 ID 和存量无损，C 处理目标熔断与未命中语义，D 做表单引用/能力/信息层次，E 才做隔离原生与少量授权真实请求。每阶段均有对应行为验收和回退边界，且保护既有未提交 OAuth 工作；没有将修复提案误写成已实施结果。

小型路由结果将逻辑聚合、实际目标、匹配方式、原模型与可选出站模型分开，能直接解决 M03/M05 的来源歧义；稳定身份与排序分离、保存前迁移预览和备份能覆盖 M01/M02；SlotId/ProviderId/拒绝三种语义明确覆盖 M06。继续复用已有供应商和适配器、两客户端保留各自目录形式，范围合理。成本判断未给未经基准或账单支持的百分比，也未引入不必要的缓存/动态调度框架。

后续执行仍须把 half-open 获取/释放、取消、普通官方直连和旧配置恢复列入实测；这已在原计划中。A/B/C 的产品回归和 E 的原生/真实账号验收仍待实施授权及实际证据。本评估及本独立审查不授权更改账号、联网调用、合并或发布。

## 完成记录

本审查已在与实施者分离的真实调度上下文执行，保存完整报告后生成 orgctl review 输入 JSON。最终校验再次核对冻结 zip、HEAD、dirty forwarder 与 14 个包内产品源码，结果一致。review_result passed 对应评估 artifact；review execution status succeeded 表示此次审查实际完成。控制者仍需执行真正的导入与 finish；本审查未调用二者。
