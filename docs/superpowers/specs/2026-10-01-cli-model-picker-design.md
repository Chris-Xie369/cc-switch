# CLI 模型选择器（modelPicker）设计 — B：CLI 侧聚合

日期：2026-10-01 · 状态：设计定稿（自主会话执行，用户审批门后置到交付复核）
前置调查：`.superpowers/sdd/cli-model-discovery.md`（机制）、`settings-key-inventory.md`（键所有权）

## 1. 目标

聚合供应商的槽位模型自动出现在 Claude Code CLI 的 `/model` 选择器里：标签可读
（「供应商 · 上游模型」），选中后模型 ID 经既有聚合路由打到正确上游。
Desktop 侧已有 `inferenceModels`，本设计补 CLI 侧的对应物 `modelPicker`。

非目标：不动代理路由（零代理改动）；不做普通供应商的行生成；不改 `env`
（终端 CLI 指哪是用户/供应商切换的事）。

## 2. 机制事实（调查结论，不再验证）

- CLI 从 managed / `--settings` / **user settings** 读 `modelPicker`，最高优先级源
  全量取代、不跨源合并；不读 project checkout。
- 行 schema：`{model, label?, description?, behavesAs?}`；逐行容错（坏行只丢自己）。
- 未知模型 ID 的行默认**不列出**，需 `behavesAs` 指向本版 CLI 认识的模型 ID。
- `replaceBuiltInOptions: true` 会连内置档位行一起隐藏。
- settings.json 的反向白名单合并（`d6b7719f`）把 `modelPicker` 列为非 owned
  顶层键 → 供应商切换间原样保留。

## 3. 设计决策

### D1. 谁写：cc-switch 在聚合供应商**应用时**自动生成写入（否决独立命令）

- 生成源与 profile `inferenceModels` 同一条 `aggregate_model_routes()`（同序、同过滤）。
- 写入点 = `apply_provider`（Claude Desktop live 写入的唯一漏斗，
  `live.rs:860` 调用它；切换/更新供应商都走这里），与 profile 同寿命。
- 备选「独立命令/按钮手动同步」否决：槽位改动后易忘同步造成漂移，且与
  `inferenceModels` 的自动同步模式不一致。

### D2. `replaceBuiltInOptions: false`（追加，不取代）

- 备选 `true`（Desktop 式纯净列表）否决：连带隐藏内置档位行，回归终端 CLI
  现有体验（终端靠 `ANTHROPIC_DEFAULT_*_MODEL` 钉内置档位，砍掉即倒退）。
- `false` 下聚合行追加在内置行之后，零回归；要纯净体验改一个常量即可。

### D3. 键所有权：cc-switch 写入的版本**可证明**才删除（不误删手写配置）

- 写入时把生成值存 DB settings（`cli_model_picker_generated`）。
- 非聚合应用（普通/官方）→ 仅当文件当前值 == 存证值才删键；不匹配（用户手写
  或改过）→ 不动。存证清空。
- 备选「无条件删除（镜像 profile inferenceModels 的 gateway-owned 语义）」否决：
  settings.json 是用户领地，d6b7719f 刚为保留用户键打过补丁，误删手写配置
  是数据损失。`inferenceModels` 的类比不成立——profile 历来全是 cc-switch 写的，
  settings.json 不是。

### D4. 行生成规则

- 每槽位一行：`{model: route_id, label: label_override?（空则省略）, behavesAs: 档位表}`。
- `supports_1m` 槽位追加 1M 行：`model: route_id + "[1m]"`，label 加后缀 ` · 1M`
  （Desktop 做不到的名字后缀，CLI 侧 label 由我们控制，顺带回应 09-24 用户诉求）。
  1M 行紧随本体行。
- behavesAs 档位表（CLI 2.1.284 二进制目录取证，高频已知 ID）：

  | tier | behavesAs | 依据 |
  |---|---|---|
  | fable | `claude-fable-5` | 二进制 41 处；本会话自身在用 fable-5 族 |
  | opus | `claude-opus-4-8` | 64 处；二进制 schema 文档自带示例；避开 opus-5（disallowThinkingDisabled） |
  | sonnet | `claude-sonnet-4-6` | 60 处 |
  | haiku | `claude-haiku-4-5` | 32 处 |

- 顺序 = `aggregate_model_routes()` 输出序（供应商分组 × 档位、defaultModel 置顶）。
- `model` 字段发给代理后由 `resolve_target` 剥 `[1m]` 查槽位，既有路由零改动。

### D5. 失败语义：衍生件尽力而为

- `sync_at` 本身严格返回 `Result`（可测）；`apply_provider` 接线处失败
  `log::warn` 降级不阻断 profile 写入（profile 是主件，picker 是衍生件，
  下次 apply 自愈）。
- settings.json **写路径**：读失败（parse/IO）**上抛、不写**——读不到就写会
  整份覆盖用户配置（AV/编辑器短暂锁文件时读失败但写成功 → 全丢），与 D3
  「不误删用户配置」一致（审查 Issue 3 后修订，2026-10-01 用户裁决）。
  文件不存在 → 创建。
- settings.json **删路径**：读失败按空对象处理（结果必然是 no-op，保守无害）；
  文件不存在 → 空操作。

## 4. 数据流

```
聚合槽位 meta.aggregateRoutes
  └─ aggregate_model_routes()          （既有，同 inferenceModels 源）
       └─ apply_provider_to_paths_inner → 返回 Option<Vec<ResolvedModelRoute>>
            └─ apply_provider（包装层）
                 ├─ profile.inferenceModels（既有，merge_profile）
                 └─ model_picker::sync_cli_model_picker(routes)
                      ├─ Some → 生成 modelPicker 写 ~/.claude/settings.json + 存证 DB
                      └─ None → 存证匹配才删键
```

Claude Code CLI（Desktop 内嵌/终端）读 user settings → `/model` 选择器出聚合行；
选中后 `body.model = route_id[（[1m]）]` → Desktop 网关/本地代理 → 聚合路由分流。

## 5. 测试计划

1. 生成纯函数：行形状、1M 追加与后缀、label 省略、behavesAs 四档映射、顺序保持、
   `replaceBuiltInOptions:false`。
2. `sync_at`：Some 写入且保留邻键；Some 覆盖旧值；None+存证匹配 → 删键保邻键；
   None+手写值不匹配 → 不动；None+无存证 → 不动。
3. 接线：`apply_provider_to_paths` 返回值（聚合 Some / 直连、官方 None）；
   `apply_provider` 全局集成（TempHome + `#[serial]`，断言 settings.json 落盘）。

## 6. 已知限制（记录，不阻塞）

- 终端 CLI 的 `env.ANTHROPIC_BASE_URL` 若指直连上游，聚合行在该上游不可用
  （选择报错）；完整体验需 BASE_URL 指向本地网关（`/claude-desktop` + 网关 token）。
  Desktop 内嵌 CLI 无此问题（宿主网关注入，本会话即证）。
- `[1m]` 行能否被该版 CLI 选择器列出取决于它对 `[1m]` 模型字段的接受度；
  逐行容错下最坏情况只是 1M 行被忽略。留用户验收确认。
- 非聚合供应商不生成行（D3 移除逻辑接管）。
