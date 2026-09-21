# 本地维护文档（快照）

本目录是 **`D:\Workspace\Project\cc-switch\` 工作区文档的快照**，随代码一起放在 fork 里，目的是**得到版本控制与异地备份**——这些"为什么这么做"的记录原先只存在于本地磁盘，代码有备份而它们没有。

## 真实来源（source of truth）

**原件在工作区，不在这个目录。** 要修改请改工作区里的原件，然后刷新快照：

```bash
cd "D:/Workspace/Project/cc-switch"
cp UPSTREAM-SYNC.md src/docs/local-maintenance/
cp doc/claude-desktop-3p-profile-overwrite-report.md doc/cc-switch-fix-brief.md src/docs/local-maintenance/
cp tools/install-local.bat src/docs/local-maintenance/
cp CLAUDE.md src/docs/local-maintenance/WORKSPACE-NOTES.md
cd src && git add docs/local-maintenance && git commit -m "docs: 刷新本地维护文档快照" && git push origin fix/profile-merge
```

## 各文件

| 文件 | 原件位置 | 内容 |
|---|---|---|
| `UPSTREAM-SYNC.md` | 工作区根 | **维护 playbook**：补丁清单（A/B/C/D 及各自上游化状态）、merge 同步流程、冲突热点、构建与部署命令、推送通道的坑 |
| `WORKSPACE-NOTES.md` | 工作区根 `CLAUDE.md` | 工作区导航：资产清单、问题根因、静态限制 vs 动态事故的区分、验收口径 |
| `claude-desktop-3p-profile-overwrite-report.md` | `doc/` | 事故报告：三层根因、证据坐标、方案对比 |
| `cc-switch-fix-brief.md` | `doc/` | 技术简报：补丁内容、测试、构建步骤 |
| `install-local.bat` | `tools/` | 覆盖安装脚本（必须经它调用，Git Bash 会转写 `/S` 与 `/D=`） |

> 工作区那份 `CLAUDE.md` 在此处改名为 `WORKSPACE-NOTES.md`：上游 `.gitignore` 第 10 行忽略了 `CLAUDE.md`（让贡献者的 AI 指令文件只留在本地），改名可避免与上游约定冲突。

## 注意

- 文档内的**路径引用写的是工作区布局**（如 `src-tauri/...`、`tools/install-local.bat`），在本仓库上下文里需自行对应。
- 这些是**纯本地文档，绝不推送到 upstream**（同分支约定：`fix/profile-merge` 只推 fork）。
- 其他文档（设计文档、实施计划、执行账本 `progress.md`）随代码一起在仓库里（`docs/superpowers/`）或属 SDD 本地草稿（`.superpowers/`，被 git 忽略）。
