# 聚合编辑器 v3：默认模型置顶 + 拖拽排序 + 折叠 + 按需档位行（2026-09-24）

## 背景与目标

编辑器 v2（供应商卡 + 固定四档行，`2026-09-23-aggregate-provider-card-editor-design.md`）验收后，
用户提出四点改进。本文是 v3 的设计定稿，已与用户对齐三个关键决策：

> **已确认**：① 默认模型 = 独立下拉（方案 B），可固定任意槽、不动卡片顺序
> ② 「默认目标」UI 改名「兜底目标」 ③ 卡片默认全折叠，支持全部展开与单卡展开

四点诉求与对策：

| 诉求 | 对策 |
|---|---|
| 控制 Claude Desktop 的启动默认模型 | 新增 `defaultModel` 槽位引用，写 profile 时置顶 |
| 重排供应商（拖拽） | 复用 `@dnd-kit`（`ProviderList.tsx` 既有模式） |
| 卡片可折叠 | 卡头折叠开关 + 摘要，默认全折叠 |
| 只显示已映射的档位行，按需添加 | 固定四行 → 增量添加（数据结构不变） |

## 关键机制（决定设计的三个事实）

1. **默认模型 = profile `inferenceModels` 第一条**（schema 原文 "The first entry is
   the default model"）。`alwaysStartWithDefaultModel=true` 时每个新会话从它起步。
2. **「默认目标」是路由兜底，不能占用**：Claude 内部调用（子代理、预热探测）请求的
   模型不在槽位表里时发给它。改成"默认模型"会丢安全网 → 只改 UI 文案，结构不动。
3. **`@dnd-kit/core|sortable|utilities` 已在依赖里**，`ProviderList.tsx` 是现成参考
   （DndContext + closestCenter + SortableContext + useSortable + 拖拽把手）。

## 数据模型

```ts
// types.ts
export interface AggregateRoutes {
  slots: AggregateRouteSlot[];
  defaultTarget: DefaultTarget;   // 结构不动；UI 文案改「兜底目标」
  /** 默认模型（槽位 routeId）。未设置 = 跟随排序首位。 */
  defaultModel?: string;
}
```

- Rust `AggregateRoutes` 加 `#[serde(default)] pub default_model: Option<String>`；
  旧数据无此字段 → 反序列化为 None → 行为与今天一致，**零迁移**。
- `defaultModel` 只能引用槽（供应商不构成"一个模型"），故是裸字符串而非 DefaultTarget 形状。

### 语义细则

- **置顶（后端 `aggregate_model_routes`）**：dedup 后若 `defaultModel` 命中某
  route_id，把该 route 移到最前，其余保持供应商分组序。悬空（引用已删槽）→ 忽略、
  顺序不变（debug 日志）。profile 与 `/v1/models` 同源同序，自然一致。
- **重编号跟随（前端 `assignSlotIds`）**：与 `DefaultTarget::SlotId` 相同的按位置
  跟随；被引用槽被删 → `defaultModel` 置 undefined（静默清空，不做保存拦截——它只
  影响启动默认，不影响路由正确性）。
- **与拖拽解耦**：方案 B 下拖动卡片不改默认；仅当 `defaultModel` 未设置时，默认 =
  首卡首个槽（现状行为）。
- **列表显示后果（用户已知情）**：置顶槽在 Claude Desktop 选择器里脱离其供应商组
  排最前，其余按供应商分组——分组被"默认槽"打破一次是方案 B 的固有代价。

## 界面

```
[兜底目标（未命中槽位时）▾]  [默认模型 ▾  未设置：跟随排序首位]
[全部展开] [全部折叠]
┌ ✥ [Zhipu GLM ▾] [获取模型列表] [🗑] [▸] ── 2 个模型 ──────┐
│  （展开后）档位 | 上游模型 | 显示名 | 1M | ×                │
│  [+ 新增模型 ▾]（只列未占用档位；满 4 档隐藏）               │
└──────────────────────────────────────────────────────────┘
[+ 新增供应商]
```

- **默认模型下拉**：选项 = 全部已映射槽（`显示名 (routeId)`，与兜底目标的槽选项同
  格式）+「未设置」空选项；placeholder「未设置：跟随排序首位」。
- **生效默认徽标**：当前生效的默认槽（固定值或首位继承）行上挂小「默认」徽标，
  所见即所得。
- **拖拽**：`useSortable` 包卡片，把手在卡头最左（✥）；`onDragEnd` → arrayMove →
  既有 `commitCards`（展平 + assignSlotIds + onChange）路径。空卡（pendingProviders）
  同样可拖。
- **折叠**：组件内 `collapsed: Set<providerId>`，初始含全部卡 id（默认全折叠）。
  折叠态 = 卡头 + 「N 个模型」摘要（N=该卡槽数）；档位行整体隐藏。「全部展开/全部
  折叠」按钮清空/填满集合。状态不持久化（每次打开表单重置为全折叠）。
- **按需档位行**：
  - 卡内只渲染已映射档位的行；行尾 × 删除该槽（替代旧「清空模型即删」语义）。
  - 「+ 新增模型」弹出档位选择（TIER_ROW_ORDER 里未被占用的档），选中即建
    `upstreamModel: ""` 的槽（与旧行为一致：空模型槽可保存、后端 profile 时丢弃）。
  - 满 4 档时按钮隐藏。新增供应商 = 空卡（既有 pendingProviders 机制复用）。
  - 行内**不支持改档位**（删了重加），卡内行序恒为 fable→opus→sonnet→haiku。

## 波及面

| 文件 | 改动 |
|---|---|
| `src/components/providers/forms/AggregateProviderFields.tsx` | 主体重构：DnD 包裹、折叠态、按需行、默认模型下拉、兜底目标改名 |
| `src/types.ts` + `src-tauri/src/aggregate.rs` | `defaultModel` 字段；置顶逻辑约 30 行 |
| `src/utils/aggregateRoutes.ts` | `assignSlotIds` 对 defaultModel 的跟随/清空 |
| `src/i18n/locales/{zh,en}.json` | defaultTarget 文案改「兜底目标（未命中槽位时）」；新增 defaultModel / placeholder / expandAll / collapseAll / addModel / modelsSummary / badge 等键 |

**不碰**：`resolve_target` 与代理路由、profile 写入语义（合并语义补丁 A）、
`flattenProviderGroups`/`groupSlotsByProvider`、后端普通供应商路径、既有 12 槽数据
（无新字段 = 行为不变）。

已知关联缺口（本次不做）：`maxEffort` 仍由脚本直写 profile，UI 保存会抹掉
（账本 2026-09-24 有记录）。

## 测试

- **TS（aggregateRoutes.test.ts）**：
  - `assignSlotIds`：defaultModel 引用的槽重编号后跟随；被删 → 置空。
- **Rust（aggregate.rs tests）**：
  - 置顶：命中 → 该 route 最前、其余分组序不变；悬空 → 顺序不变；未设置 → 原序。
  - serde 兼容：无 defaultModel 的旧 JSON 反序列化为 None。
- **实机验收**：
  1. 设默认模型 → profile 首条 = 该槽，Claude Desktop 重开选择器默认位变化；
  2. 拖拽换卡序 → 保存后 `/v1/models` 顺序跟随；默认（未设置时）= 新首卡首槽；
  3. 折叠/展开/全部展开；摘要计数正确；
  4. 增删档位行、满 4 档隐藏按钮、空卡保存不丢；
  5. 全量 12 槽位回归 200。

## 实施顺序

1. Rust：字段 + 置顶 + 测试（先红后绿）
2. TS：类型 + `assignSlotIds` 跟随 + 测试
3. 组件重构（DnD → 折叠 → 按需行 → 默认模型下拉，分步可编译）
4. i18n；typecheck + vitest 全绿
5. 构建部署（UPSTREAM-SYNC §5–§6）+ 实机验收 + 账本

---

## 实施记录（2026-09-25）

- 代码提交 `0608a231`（本 spec `e84b9bbf`），工作区干净，构建部署中。
- 四点全落地；「兜底目标」文案落地。
- 测试：Rust aggregate 相关全绿（3 个新用例 pins/dangling/serde + 25 既有）；
  typecheck ✓；vitest 29 例 ✓。
- ⚠️ 断言随语义更新：`claude_desktop_config.rs` 的
  `aggregate_provider_derives_model_routes_from_slots` 从「字典序」改「保 provider
  分组序」——它测的正是 09-24 已替换的旧行为，非回归。
- 22 处 AggregateRoutes 字面量构造补 `default_model: None`（跨行 SlotId 两处曾误插、已修）。
