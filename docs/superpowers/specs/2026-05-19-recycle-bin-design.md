# 回收站设计

## 背景

CodexPilot 现在已经支持删除会话，并在删除时把 SQLite 相关记录和 rollout 文件写入 `.codex-pilot-undo` 备份。当前恢复入口主要依赖删除后的短时 Toast，刷新页面或过一段时间后用户很难再找到可恢复记录。

回收站要把这些 undo 备份变成 Manager 里的持久管理入口。它负责列出已删除会话、恢复选中记录、永久删除备份记录。

## 目标

- Manager 的“对话维护”页面内新增“回收站”区域。
- 列出 `.codex-pilot-undo` 中可识别的备份记录。
- 支持选择单条或多条记录。
- 支持“恢复所选”，复用现有 undo 恢复逻辑。
- 支持“永久删除”，删除对应 undo 备份文件，删除后不能再恢复。
- 所有可见文案使用中文。
- 不展示会话正文、消息内容、用户输入、token 或 API Key。

## 非目标

- 不把回收站放进 Codex 页面浮动菜单。
- 不在永久删除时再次操作 Codex SQLite。会话进入回收站时已经从 SQLite 中删除，回收站永久删除只删除 undo 备份。
- 不读取或展示完整会话内容。
- 不做自动清理策略或定时删除。
- 不做跨设备同步。

## 数据模型

新增回收站列表项：

```text
RecycleBinEntry
- token: string
- session_id: string
- title: string | null
- project_cwd: string | null
- schema: string
- db_path: PathBuf
- backup_path: PathBuf
- deleted_at: number | null
- last_active_at: number | null
- recoverable: boolean
- status: string
```

`token` 来自备份文件名。`deleted_at` 优先使用备份文件修改时间。`title` 只从备份里的 `threads.title` 或 `sessions.title` 读取，不读取消息正文。取不到标题时 UI 显示“未命名会话”或短会话 ID。
`project_cwd` 从 `threads.cwd` 读取；`last_active_at` 优先从 `updated_at_ms`、`updated_at`、`created_at_ms` 推导。取不到时 UI 显示 `-`。

undo 备份中允许包含内部辅助段：

```text
__session_index
- path: string
- line: string
```

该段只用于恢复 Codex 自身的 `session_index.jsonl` 列表索引。`line` 保存匹配被删 thread id 的原始 JSONL 行，不展示给 UI。

## 后端设计

在 `codex-pilot-data::storage` 增加：

- `list_undo_backups() -> anyhow::Result<Vec<RecycleBinEntry>>`
- `delete_undo_backup(token: &str) -> anyhow::Result<DeleteResult>`

列表读取只扫描当前数据库对应的 backup dir。读取失败、JSON 损坏、db_path 不匹配等情况不直接崩溃，而是在条目上标记不可恢复或跳过无法识别文件。永久删除会校验 token，解析路径必须落在 backup dir 内。

在 bridge/core 层增加路由：

- `/session/recycle-bin/list`
- `/session/recycle-bin/restore`
- `/session/recycle-bin/delete`

Manager Tauri command 对应增加：

- `recycle_bin_snapshot`
- `restore_recycle_bin_entries`
- `delete_recycle_bin_entries`

恢复成功后继续执行 provider sync，沿用现有 undo 行为。

删除 Codex thread 时，除了删除 SQLite 行和 rollout 文件，还要同步移除 `~/.codex/session_index.jsonl` 中对应 thread id 的索引行。移除前把原始行写入 undo 备份的 `__session_index`。恢复时，在 SQLite 和 rollout 文件恢复成功后，把备份的索引行写回 `session_index.jsonl`；写回前按 id 去重，避免重复索引。

`session_index.jsonl` 不存在、缺少对应行，或备份里没有 `__session_index` 时，不阻塞删除和恢复。索引同步失败时，删除/恢复返回失败并保留 undo 备份，避免数据库状态和 Codex 列表长期不一致。

## UI 设计

Manager 左侧导航使用后续布局设计中的“对话维护”。该页面内的“回收站”区域结构：

- 顶部摘要：已删除记录数量、选中数量。
- 操作按钮：`刷新`、`恢复可恢复项`、`永久删除`。
- 表格列：选择框、标题、来源、最后活跃、删除时间、状态。
- 回收站表格主体设置最大高度并内部滚动，表头固定，避免记录过多时把下方“对话归属同步”区域挤出视口。
- 表格行保持单行紧凑高度；标题、来源、时间、状态列使用固定宽度和省略号，鼠标悬浮显示完整信息。
- 来源列显示项目目录名，完整 `cwd` 放在悬浮信息中；不再把 `codex_threads` 这类 schema 技术名作为来源主文案。
- 会话 ID 和备份路径不作为主列展示，只保留在悬浮信息中用于排查。
- 空状态：`暂无已删除会话`。
- 错误状态：显示中文错误，不暴露敏感路径以外的内容。

交互规则：

- 未选中记录时，恢复和永久删除按钮禁用；如果选中项里没有可恢复记录，恢复按钮禁用。
- 恢复只处理已选中的可恢复记录；不可恢复记录可以被选中用于永久删除，但不参与恢复。
- 恢复和永久删除可以批量执行；每条记录独立返回结果，最后刷新列表。
- 永久删除前弹出确认：`确认永久删除选中的 N 条记录？删除后不能恢复。`
- 恢复或永久删除部分失败时，成功处理的记录从选择中移除，失败记录保持选中，并显示失败原因。

## 安全与隐私

- 列表只显示标题、session_id、schema、删除时间和备份路径。
- 不展示 conversation message、rollout 内容、用户输入、模型输出、token。
- 诊断日志记录 token 数量和操作结果，不记录备份正文。
- 永久删除只接受 token，不接受任意文件路径。

## 测试计划

- `codex-pilot-data` fixture：删除会话后能列出回收站条目。
- `codex-pilot-data` fixture：删除 Codex thread 会移除 `session_index.jsonl` 中对应索引，undo 会恢复索引。
- `codex-pilot-data` fixture：损坏备份不会导致整个列表失败。
- `codex-pilot-data` fixture：永久删除只删除目标备份文件。
- `codex-pilot-data` fixture：恢复后条目仍可按现有 undo 逻辑恢复数据。
- `codex-pilot-core` route tests：列表、恢复、永久删除的响应 shape。
- `codex-pilot-manager` command tests：批量恢复和批量永久删除聚合中文结果。
- 前端检查：`npm run check`、`npm run vite:build`。

## 实施顺序

1. 在 data 层实现回收站条目解析、列表、永久删除。
2. 增加 core route 和 Manager Tauri command。
3. 在 Manager “对话维护”页面新增回收站区域。
4. 补测试并跑完整验证。
5. 更新路线图 checklist 状态。
