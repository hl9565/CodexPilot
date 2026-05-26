# `storage.rs` 拆分映射报告

更新时间：2026-05-26  
目标文件：[`crates/codex-pilot-data/src/storage.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/storage.rs)  
当前文件长度：2203 行（与 backlog 中“2122 行”的描述相比，当前仓库实值已增长）

## 0. 适用范围与结论摘要

这份报告补完 T09 没覆盖的内容，覆盖：

- 所有顶层非 `pub` 函数，以及顶层 `pub(crate) fn` / `pub(super) fn`
- 所有非 `pub` struct / enum
- 所有文件级 `const` / `static`
- `#[cfg(test)] mod tests` 内所有测试函数与测试 fixture 的归属
- 为 T20b 机械拆分准备的 `pub(super)` 升级清单

本报告只做映射，不改 [`storage.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/storage.rs) 实现，也不新建任何 `.rs` 文件。

## 1. 目标文件结构

建议拆成下面这组子模块：

```text
crates/codex-pilot-data/src/storage/
├── mod.rs
├── models.rs
├── recycle_bin.rs
├── delete_undo.rs
├── codex_threads.rs
├── schema.rs
├── backup.rs
└── sql_helpers.rs
```

各文件职责建议如下：

- `mod.rs`
  - 只做 `mod` 声明、扁平 `pub use`、极少量 `pub(crate)` re-export
  - 保留跨组集成测试
  - 不保留运行时逻辑
- `models.rs`
  - `SessionRef`
  - `DeleteStatus`
  - `DeleteResult`
  - 删除结果构造 helper：`deleted` / `not_found` / `failed` / `failed_with_backup`
- `recycle_bin.rs`
  - `RecycleBinEntry`
  - `SQLiteStorageAdapter::{list_undo_backups, delete_undo_backup}`
  - `recycle_entry_from_path`
- `delete_undo.rs`
  - `SQLiteStorageAdapter::{delete_local, inspect_delete_local, undo}`
  - `delete_generic_session`
  - `delete_codex_thread`
  - 只服务删除/撤销主流程的 helper：`select_rows` / `backup_related_rows` / `delete_related_rows` / `sample_thread_ids`
- `codex_threads.rs`
  - `SQLiteStorageAdapter::{find_archived_thread_by_title, move_codex_thread_workspace, codex_thread_sort_key, codex_thread_sort_keys}`
  - 线程时间戳、rollout fallback、workspace 回写 helper
  - `MAX_SORT_KEY_BATCH`
- `schema.rs`
  - `SchemaKind`
  - `schema_kind`
  - `has_table`
  - `has_columns`
  - `table_columns`
- `backup.rs`
  - `BackupPayload`
  - `SQLiteStorageAdapter::{write_backup, backup_path}`
  - 回收站 JSON 读写、表恢复、rollout 文件备份恢复、session index 备份恢复、备份元数据读取 helper
- `sql_helpers.rs`
  - `OwnedSqlValue`
  - `sql_value_to_json`
  - `json_to_sql_value`
  - `quote_identifier`
  - `sanitize_token_part`
  - `encode_hex`
  - `decode_hex`
  - `hex_digit`

选择这套结构的理由：

- 它和 T09 的 A/B/C/D 分组基本一致，但把“共享底座”进一步拆成 `schema.rs`、`backup.rs`、`sql_helpers.rs` 三层，能避免把 `delete_undo.rs` 重新长成第二个大杂烩。
- `backup.rs` 单独存在是必要的，因为 `undo`、回收站条目解析、thread 删除后的文件/index 备份，三者共用同一份内部备份协议。
- 不再单独引入 `test_support.rs`。T20b 第一轮应先减少模块数，测试支撑函数先留在 `mod.rs` 测试区更稳。

## 2. 顶层非 `pub` 函数归属表

说明：

- 本表严格按用户要求，只列“顶层” `fn` / `pub(crate) fn` / `pub(super) fn`，不含 `impl` 块里的方法。
- callers 按当前 [`storage.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/storage.rs) 实际调用关系整理。
- “建议可见性”是拆分后的目标状态，不必等同当前状态。

| 函数名 | 当前行号 | 当前可见性 | callers | 建议归属 | 建议可见性 | 备注 |
| --- | ---: | --- | --- | --- | --- | --- |
| `sample_thread_ids` | 742 | `fn` | `inspect_delete_local` | `delete_undo.rs` | `fn` | 只服务删除预检，和 `inspect_delete_local` 强耦合。 |
| `schema_kind` | 755 | `pub(crate)` | `delete_local`、`inspect_delete_local`、`find_archived_thread_by_title`、`move_codex_thread_workspace`、`codex_thread_sort_key`、`codex_thread_sort_keys` | `schema.rs` | `pub(super)` | 被 C/D 两组共享，是标准的 storage 内部公共 helper。 |
| `normalize_session_id` | 768 | `pub(crate)` | `SessionRef::normalized_id` | `models.rs` | `fn` | 只被 `SessionRef` 包一层调用，拆后可收窄为模型模块私有。 |
| `has_table` | 775 | `pub(crate)` | `delete_generic_session`、`delete_codex_thread`、`sample_thread_ids`、`schema_kind`、`backup_related_rows`、`delete_related_rows`、`validate_restore_tables`、`update_existing_agent_job_item` | `schema.rs` | `pub(super)` | C 组和 `backup.rs` 都依赖它。 |
| `has_columns` | 785 | `pub(crate)` | `find_archived_thread_by_title`、`move_codex_thread_workspace`、`delete_codex_thread`、`schema_kind` | `schema.rs` | `pub(super)` | C/D 两组共享。 |
| `table_columns` | 790 | `fn` | `has_columns`、`select_rows`、`validate_restore_tables`、`codex_thread_timestamp_columns` | `schema.rs` | `pub(super)` | 底层 schema 反射 helper，跨 `schema.rs` / `delete_undo.rs` / `codex_threads.rs`。 |
| `select_rows` | 799 | `fn` | `inspect_delete_local`、`delete_generic_session`、`delete_codex_thread`、`backup_related_rows` | `delete_undo.rs` | `fn` | 只在 C 组使用，留在删除编排模块最自然。 |
| `backup_related_rows` | 828 | `fn` | `delete_codex_thread` | `delete_undo.rs` | `fn` | 只服务 codex thread 删除备份。 |
| `delete_related_rows` | 844 | `fn` | `delete_codex_thread` | `delete_undo.rs` | `fn` | 只服务 codex thread 删除主流程。 |
| `restore_tables` | 860 | `fn` | `undo` | `backup.rs` | `pub(super)` | `undo` 在 `delete_undo.rs`，实现细节在 `backup.rs`，需要父模块可见。 |
| `restore_table_order` | 885 | `fn` | `restore_tables` | `backup.rs` | `fn` | 只服务恢复顺序，和 `restore_tables` 一起搬。 |
| `validate_restore_tables` | 914 | `fn` | `restore_tables` | `backup.rs` | `fn` | 只服务恢复前校验。 |
| `insert_row` | 940 | `fn` | `restore_tables` | `backup.rs` | `fn` | 只服务表恢复，和 `OwnedSqlValue` / `json_to_sql_value` 紧耦合。 |
| `update_existing_agent_job_item` | 973 | `fn` | `insert_row` | `backup.rs` | `fn` | 是 `insert_row` 的分支规则，不应独立暴露。 |
| `rollout_file_backups` | 1005 | `fn` | `delete_codex_thread` | `backup.rs` | `pub(super)` | 逻辑属于备份协议，但由删除编排调用。 |
| `remove_rollout_files` | 1020 | `fn` | `delete_codex_thread` | `backup.rs` | `pub(super)` | 和 rollout 备份恢复是一组，建议统一落在 `backup.rs`。 |
| `restore_files` | 1035 | `fn` | `undo` | `backup.rs` | `pub(super)` | `undo` 需要。 |
| `session_index_backups` | 1061 | `fn` | `delete_codex_thread` | `backup.rs` | `pub(super)` | index 备份是 undo 协议的一部分。 |
| `remove_session_index_entries` | 1079 | `fn` | `delete_codex_thread` | `backup.rs` | `pub(super)` | 删除流程调用，但本质是 undo 配套文件操作。 |
| `restore_session_index_entries` | 1118 | `fn` | `undo` | `backup.rs` | `pub(super)` | `undo` 需要。 |
| `session_index_path` | 1172 | `fn` | `session_index_backups` | `backup.rs` | `fn` | 只服务 session index helper。 |
| `session_index_line_id` | 1179 | `fn` | `session_index_backups`、`remove_session_index_entries`、`restore_session_index_entries` | `backup.rs` | `fn` | index helper 内部公共函数。 |
| `update_rollout_session_meta_cwd` | 1187 | `fn` | `move_codex_thread_workspace` | `codex_threads.rs` | `fn` | 只服务 D 组 workspace 迁移。 |
| `codex_thread_timestamp_columns` | 1228 | `fn` | `move_codex_thread_workspace`、`fetch_thread_timestamp_payload` | `codex_threads.rs` | `fn` | D 组内部共享。 |
| `fetch_thread_timestamp_payload` | 1239 | `fn` | `codex_thread_sort_key`、`codex_thread_sort_keys` | `codex_threads.rs` | `fn` | D 组内部共享。 |
| `sessions_dir_for` | 1272 | `fn` | `codex_thread_sort_key`、`codex_thread_sort_keys` | `codex_threads.rs` | `fn` | 只服务 rollout fallback 查找。 |
| `collect_rollout_timestamps` | 1279 | `fn` | `codex_thread_sort_keys`、`rollout_fallback_timestamp_payload` | `codex_threads.rs` | `fn` | D 组内部共享。 |
| `walk_rollout_dir` | 1288 | `fn` | `collect_rollout_timestamps` | `codex_threads.rs` | `fn` | rollout fallback 细节，留私有。 |
| `days_from_civil_utc` | 1315 | `fn` | `parse_rollout_filename` | `codex_threads.rs` | `fn` | 只服务 rollout 文件名解析。 |
| `parse_rollout_filename` | 1325 | `fn` | `walk_rollout_dir`，测试：`parse_rollout_filename_extracts_id_and_timestamp`、`parse_rollout_filename_rejects_garbage` | `codex_threads.rs` | `fn` | 纯工具函数，跟测试一起搬到 D 组。 |
| `rollout_fallback_timestamp_payload` | 1361 | `fn` | `codex_thread_sort_key`，测试：`rollout_fallback_finds_existing_file`、`rollout_fallback_returns_none_for_missing_id` | `codex_threads.rs` | `fn` | D 组纯 helper。 |
| `add_timestamp_payload` | 1374 | `fn` | `move_codex_thread_workspace`、`fetch_thread_timestamp_payload` | `codex_threads.rs` | `fn` | D 组内部共享。 |
| `backup_title` | 1383 | `fn` | `recycle_entry_from_path` | `backup.rs` | `pub(super)` | 回收站条目解析需要；建议和备份元数据 helper 一起放。 |
| `backup_project_cwd` | 1398 | `fn` | `recycle_entry_from_path` | `backup.rs` | `pub(super)` | 同上。 |
| `backup_last_active_at` | 1407 | `fn` | `recycle_entry_from_path` | `backup.rs` | `pub(super)` | 同上。 |
| `backup_first_row` | 1414 | `fn` | `backup_project_cwd`、`backup_last_active_at` | `backup.rs` | `fn` | 只服务备份元数据 helper。 |
| `timestamp_seconds` | 1425 | `fn` | `backup_last_active_at` | `backup.rs` | `fn` | 只服务备份元数据 helper。 |
| `parse_rfc3339_seconds` | 1452 | `fn` | `timestamp_seconds` | `backup.rs` | `fn` | 同上。 |
| `days_from_civil` | 1478 | `fn` | `parse_rfc3339_seconds` | `backup.rs` | `fn` | 同上。 |
| `token_session_id` | 1497 | `fn` | `delete_undo_backup`、`recycle_entry_from_path` | `recycle_bin.rs` | `fn` | 只服务回收站 token 解析。 |
| `sql_value_to_json` | 1505 | `fn` | `move_codex_thread_workspace`、`select_rows`、`fetch_thread_timestamp_payload` | `sql_helpers.rs` | `pub(super)` | C/D 两组共享的 SQL 值转换。 |
| `json_to_sql_value` | 1517 | `fn` | `insert_row`、`update_existing_agent_job_item` | `sql_helpers.rs` | `pub(super)` | 虽然当前只被恢复逻辑使用，但和 `OwnedSqlValue` 应收在同层。 |
| `quote_identifier` | 1541 | `fn` | `move_codex_thread_workspace`、`table_columns`、`select_rows`、`delete_related_rows`、`insert_row`、`fetch_thread_timestamp_payload` | `sql_helpers.rs` | `pub(super)` | `schema.rs`、`delete_undo.rs`、`backup.rs`、`codex_threads.rs` 都用。 |
| `sanitize_token_part` | 1545 | `fn` | `write_backup` | `sql_helpers.rs` | `pub(super)` | 虽只被 `write_backup` 调用，但建议和 token/hex 纯工具一起放。 |
| `encode_hex` | 1559 | `fn` | `rollout_file_backups`、`sql_value_to_json` | `sql_helpers.rs` | `pub(super)` | `backup.rs` 与 SQL 转换共享。 |
| `decode_hex` | 1569 | `fn` | `restore_files`、`json_to_sql_value` | `sql_helpers.rs` | `pub(super)` | 同上。 |
| `hex_digit` | 1583 | `fn` | `decode_hex` | `sql_helpers.rs` | `fn` | 只服务解码实现。 |
| `deleted` | 1592 | `fn` | `delete_generic_session`、`delete_codex_thread` | `models.rs` | `pub(super)` | 返回值构造 helper，建议归到 `DeleteResult` 同层。 |
| `not_found` | 1602 | `fn` | `delete_undo_backup`、`delete_generic_session`、`delete_codex_thread` | `models.rs` | `pub(super)` | B/C 两组共享。 |
| `failed` | 1612 | `fn` | `delete_local` | `models.rs` | `pub(super)` | 当前只有 C 组用，但与 `failed_with_backup` 同组更整齐。 |
| `failed_with_backup` | 1616 | `fn` | `undo`、`delete_codex_thread`、`failed` | `models.rs` | `pub(super)` | 删除/撤销失败返回值构造 helper。 |

## 3. 非 `pub` struct / enum 与 `const` / `static` 归属

### 3.1 非 `pub` struct / enum

| 名称 | 当前行号 | 当前可见性 | callers / users | 建议归属 | 建议可见性 | 备注 |
| --- | ---: | --- | --- | --- | --- | --- |
| `SchemaKind` | 77 | `pub(crate) enum` | `schema_kind`、`delete_local`、`inspect_delete_local`、`find_archived_thread_by_title`、`move_codex_thread_workspace`、`codex_thread_sort_key`、`codex_thread_sort_keys` | `schema.rs` | `pub(super)` | 只在 `storage` 内部做 schema 分流，没必要继续放大到整个 crate。 |
| `OwnedSqlValue` | 83 | `struct` | `insert_row`、`update_existing_agent_job_item`、`impl ToSql for OwnedSqlValue` | `sql_helpers.rs` | `pub(super)` | 如果 `insert_row` 留在 `backup.rs`，它就需要被父模块导入；否则要把 `insert_row` 也迁进 `sql_helpers.rs`，不推荐。 |
| `BackupPayload` | 92 | `struct` | `undo`、`write_backup`、`recycle_entry_from_path` | `backup.rs` | `pub(super)` | 是 B/C 共用的内部备份协议类型，T20b 最容易漏掉。 |

### 3.2 文件级 `const` / `static`

| 名称 | 当前行号 | 当前可见性 | users | 建议归属 | 建议可见性 | 备注 |
| --- | ---: | --- | --- | --- | --- | --- |
| `MAX_SORT_KEY_BATCH` | 101 | `pub(crate) const` | `codex_thread_sort_keys`、测试 `finds_archived_thread_moves_workspace_and_reads_sort_keys`、`sort_keys_truncates_at_max_batch_with_explicit_marker` | `codex_threads.rs` | `pub(super)` | 运行时只服务 D 组；如果 `sort_keys_truncates...` 跟着搬进 `codex_threads.rs`，可继续收窄为 `const`。 |

补充：

- 当前文件没有文件级 `static`。
- `encode_hex` 里的局部 `const HEX` 不属于文件级共享常量，不需要单独映射；拆分时继续留在 `encode_hex` 内部即可。

## 4. 测试归属表

建议原则：

- 纯 helper 单测跟着子模块下沉。
- 任何同时覆盖 DB + 回收站文件 + session index 的测试，先留在 `storage/mod.rs`。
- T20b 第一轮不建议新建 `storage/test_support.rs`；`unique_temp_path` 继续放 `mod.rs` 测试区即可。

| 测试名 | 当前行号 | 覆盖范围 | 建议归属 |
| --- | ---: | --- | --- |
| `unique_temp_path` | 1636 | 通用临时 DB / rollout 路径 fixture | `storage/mod.rs` 的 `#[cfg(test)]` 区；T20b 第一轮不要再拆测试支撑模块 |
| `deletes_and_undoes_generic_session` | 1647 | Generic session 删除 + 回收站列举 + undo 恢复 | `storage/mod.rs` |
| `recycle_bin_lists_corrupt_backups_and_deletes_permanently` | 1720 | 回收站损坏备份识别 + 永久删除；同时触发 `delete_local` | `storage/mod.rs` |
| `undo_restores_parent_rows_before_foreign_key_children` | 1761 | `undo` 表恢复顺序 / FK 约束 | `storage/mod.rs` |
| `deletes_codex_thread_fixture` | 1819 | Codex thread 删除 + rollout 文件删除恢复 + agent_job_items 还原 | `storage/mod.rs` |
| `deletes_and_restores_codex_session_index_entry` | 1928 | Codex thread 删除 + session index 文件更新 + undo 去重恢复 | `storage/mod.rs` |
| `finds_archived_thread_moves_workspace_and_reads_sort_keys` | 1989 | D 组公开 API：归档查找、workspace 迁移、单个/批量 sort key | `codex_threads.rs` |
| `sort_keys_truncates_at_max_batch_with_explicit_marker` | 2089 | `MAX_SORT_KEY_BATCH` 与 `codex_thread_sort_keys` 截断语义 | `codex_threads.rs` |
| `recycle_bin_entry_reads_project_and_last_active_from_thread_backup` | 2135 | 回收站条目元数据提取，依赖 thread 删除备份内容 | `storage/mod.rs` |
| `parse_rollout_filename_extracts_id_and_timestamp` | 2167 | `parse_rollout_filename` 纯函数 | `codex_threads.rs` |
| `parse_rollout_filename_rejects_garbage` | 2177 | `parse_rollout_filename` 纯函数 | `codex_threads.rs` |
| `rollout_fallback_finds_existing_file` | 2184 | `rollout_fallback_timestamp_payload` 纯 helper | `codex_threads.rs` |
| `rollout_fallback_returns_none_for_missing_id` | 2199 | `rollout_fallback_timestamp_payload` 纯 helper | `codex_threads.rs` |

测试 fixture 归属补充：

- `unique_temp_path` 被 6 个跨模块集成测试复用，建议继续留在 `storage/mod.rs`。
- `tempfile::TempDir` 只出现在纯 rollout fallback helper 测试里，跟着 `codex_threads.rs` 的局部测试使用即可。

## 5. 可见性升级清单

这一节只列“为了按上面的目录拆分后还能编译”必须升到 `pub(super)` 的项。这里不追求最少改动行数，而追求不漏项。

### 5.1 顶层函数必须升 `pub(super)`

- `schema_kind`
- `has_table`
- `has_columns`
- `table_columns`
- `restore_tables`
- `rollout_file_backups`
- `remove_rollout_files`
- `restore_files`
- `session_index_backups`
- `remove_session_index_entries`
- `restore_session_index_entries`
- `backup_title`
- `backup_project_cwd`
- `backup_last_active_at`
- `sql_value_to_json`
- `json_to_sql_value`
- `quote_identifier`
- `sanitize_token_part`
- `encode_hex`
- `decode_hex`
- `deleted`
- `not_found`
- `failed`
- `failed_with_backup`

### 5.2 非 `pub` 类型必须升 `pub(super)`

- `SchemaKind`
- `OwnedSqlValue`
- `BackupPayload`

### 5.3 `impl SQLiteStorageAdapter` 私有方法中，必须升 `pub(super)` 的项

虽然它们不在第 2 节表里，但 T20b 真拆时会直接撞到：

- `write_backup`
  - 原因：建议落在 `backup.rs`，但由 `delete_generic_session` / `delete_codex_thread` 调用。
- `backup_path`
  - 原因：`undo`、`delete_undo_backup`、`write_backup`、测试都会跨模块用到。

### 5.4 不需要升级、应继续保持模块私有的项

这组不要误升：

- `sample_thread_ids`
- `select_rows`
- `backup_related_rows`
- `delete_related_rows`
- `restore_table_order`
- `validate_restore_tables`
- `insert_row`
- `update_existing_agent_job_item`
- `session_index_path`
- `session_index_line_id`
- `update_rollout_session_meta_cwd`
- `codex_thread_timestamp_columns`
- `fetch_thread_timestamp_payload`
- `sessions_dir_for`
- `collect_rollout_timestamps`
- `walk_rollout_dir`
- `days_from_civil_utc`
- `parse_rollout_filename`
- `rollout_fallback_timestamp_payload`
- `add_timestamp_payload`
- `backup_first_row`
- `timestamp_seconds`
- `parse_rfc3339_seconds`
- `days_from_civil`
- `token_session_id`
- `hex_digit`
- `delete_generic_session`
- `delete_codex_thread`
- `recycle_entry_from_path`

## 6. 风险点补充

### 6.1 跨子模块循环依赖风险

按本报告建议拆分，主要依赖方向应为：

```text
models.rs      <- recycle_bin.rs / delete_undo.rs / codex_threads.rs
schema.rs      <- delete_undo.rs / codex_threads.rs / backup.rs
sql_helpers.rs <- delete_undo.rs / codex_threads.rs / backup.rs / schema.rs
backup.rs      <- recycle_bin.rs / delete_undo.rs
```

重点风险有两处：

- `delete_undo.rs` 与 `backup.rs`
  - `delete_undo.rs` 需要 `write_backup` / `restore_tables` / 文件和 index 备份恢复 helper。
  - `backup.rs` 不应反向依赖 `delete_undo.rs`；否则马上形成循环。
  - 处理方式：`backup.rs` 只提供“备份协议和恢复动作”，不引用删除编排函数。
- `schema.rs` 与 `sql_helpers.rs`
  - `table_columns` 需要 `quote_identifier`，所以 `schema.rs -> sql_helpers.rs` 是合理的。
  - 不要让 `sql_helpers.rs` 再去依赖 `schema.rs`。

结论：只要保持“底层 helper 单向被业务模块调用”，不会出现 A 依赖 B 同时 B 依赖 A 的必然循环。

### 6.2 外部 crate 是否在用本应私有的东西

对当前仓库实际 grep 结果：

- 发现外部 crate 直接使用的只有公开 API：
  - `codex_pilot_data::storage::SessionRef`
  - `codex_pilot_data::storage::SQLiteStorageAdapter`
- 未发现 `codex_pilot_data::storage::schema_kind`、`has_table`、`normalize_session_id`、`MAX_SORT_KEY_BATCH` 等内部 helper 被外部 crate 调用。
- 当前仓库下不存在 `crates/codex-pilot-manager` 目录；因此“检查 core 和 manager”实际只能确认 `codex-pilot-core`。

实际命中位置：

- [`crates/codex-pilot-core/src/routes.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes.rs)
- [`crates/codex-pilot-core/src/routes_sessions.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs)

结论：

- T20b 可以安全地把 `schema_kind`、`SchemaKind`、`normalize_session_id` 等从 `pub(crate)` 收窄到 `pub(super)` 或 `fn`，不会影响当前仓库内外部调用点。
- 但不要改公开 API 路径；对外仍应保持 `codex_pilot_data::storage::*`。

### 6.3 import 下沉建议

拆分后，运行时 import 基本都可以下沉到子模块；`mod.rs` 最好只保留：

- `mod ...;`
- `pub use ...;`
- `#[cfg(test)] mod tests;`

运行时 import 的推荐下沉位置：

- `schema.rs`
  - `rusqlite::{Connection, ToSql}`
  - `std::collections::HashSet`
- `sql_helpers.rs`
  - `rusqlite::types::{ToSqlOutput, Value as SqlValue, ValueRef}`
  - `rusqlite::ToSql`
  - `serde_json::{Value, json}`
- `backup.rs`
  - `serde_json::{Map, Value}`
  - `std::fs`
  - `std::path::{Path, PathBuf}`
  - `std::time::{SystemTime, UNIX_EPOCH}`
  - `rusqlite::{Connection, ToSql}`
- `delete_undo.rs`
  - `rusqlite::Connection`
  - `serde_json::{Map, Value, json}`
- `codex_threads.rs`
  - `rusqlite::Connection`
  - `serde_json::{Map, Value, json}`
  - `std::collections::{HashMap, HashSet}`
  - `std::fs`
  - `std::path::{Path, PathBuf}`
  - `std::time::UNIX_EPOCH`
- `recycle_bin.rs`
  - `std::fs`
  - `std::path::{Path, PathBuf}`
  - `std::time::UNIX_EPOCH`
  - `serde_json::Value`

必须留在 `mod.rs` 的，原则上只有测试 import：

- `use super::*;`
- `use rusqlite::Connection;`
- `use std::fs;`
- `use std::path::PathBuf;`
- `use std::time::{SystemTime, UNIX_EPOCH};`

### 6.4 其它易错点

- `MAX_SORT_KEY_BATCH`
  - 如果把 `sort_keys_truncates...` 测试留在 `mod.rs`，这个常量不能直接收窄成子模块私有。
- `BackupPayload`
  - 它不是只给 `undo` 用；回收站条目解析同样依赖它。
- `OwnedSqlValue`
  - 如果误留成私有类型而 `insert_row` 搬到别的模块，会在恢复逻辑编译时报类型不可见。
- `backup_path`
  - 不只是运行时代码，现有 4 个集成测试也直接用它断言备份文件是否删除。

## 7. T20b 执行计划

目标：严格机械搬运，不顺手优化。每一步单独 commit，每一步都跑 `cargo test --workspace`。

### 第 1 步：建立 `storage/` 目录骨架并保持对外路径不变

- 新建 `crates/codex-pilot-data/src/storage/`
- 把现有 `storage.rs` 改成 `storage/mod.rs`
- 在 `mod.rs` 先声明：
  - `mod models;`
  - `mod recycle_bin;`
  - `mod delete_undo;`
  - `mod codex_threads;`
  - `mod schema;`
  - `mod backup;`
  - `mod sql_helpers;`
- 先把全部实现暂时留在 `mod.rs`，只让路径切换成立
- `cargo test --workspace`
- 单独 commit

### 第 2 步：搬 A 组公开模型到 `models.rs`

- 搬 `SessionRef` / `DeleteStatus` / `DeleteResult`
- 一并搬 `normalize_session_id`
- 一并搬 `deleted` / `not_found` / `failed` / `failed_with_backup`
- 在 `mod.rs` 里 `pub use models::{...};`
- `cargo test --workspace`
- 单独 commit

### 第 3 步：搬 schema 反射层到 `schema.rs`

- 搬 `SchemaKind`
- 搬 `schema_kind` / `has_table` / `has_columns` / `table_columns`
- 按本报告把需要跨模块共享的项升到 `pub(super)`
- `cargo test --workspace`
- 单独 commit

### 第 4 步：搬 SQL 纯工具层到 `sql_helpers.rs`

- 搬 `OwnedSqlValue`
- 搬 `sql_value_to_json` / `json_to_sql_value` / `quote_identifier`
- 搬 `sanitize_token_part` / `encode_hex` / `decode_hex` / `hex_digit`
- 按本报告做 `pub(super)` 升级
- `cargo test --workspace`
- 单独 commit

### 第 5 步：搬备份协议与恢复层到 `backup.rs`

- 搬 `BackupPayload`
- 搬 `SQLiteStorageAdapter::{write_backup, backup_path}`
- 搬 `restore_tables` / `restore_table_order` / `validate_restore_tables` / `insert_row` / `update_existing_agent_job_item`
- 搬 rollout 文件备份恢复 helper
- 搬 session index 备份恢复 helper
- 搬 `backup_title` / `backup_project_cwd` / `backup_last_active_at` / 时间解析 helper
- 按本报告做 `pub(super)` 升级
- `cargo test --workspace`
- 单独 commit

### 第 6 步：搬 B 组回收站入口到 `recycle_bin.rs`

- 搬 `RecycleBinEntry`
- 搬 `SQLiteStorageAdapter::{list_undo_backups, delete_undo_backup}`
- 搬 `recycle_entry_from_path`
- 搬 `token_session_id`
- 让它改为依赖 `backup.rs` 中的 `BackupPayload` / 元数据 helper
- `cargo test --workspace`
- 单独 commit

### 第 7 步：搬 C 组删除与撤销主流程到 `delete_undo.rs`

- 搬 `SQLiteStorageAdapter::{delete_local, inspect_delete_local, undo}`
- 搬 `delete_generic_session`
- 搬 `delete_codex_thread`
- 搬 `sample_thread_ids` / `select_rows` / `backup_related_rows` / `delete_related_rows`
- 对照本报告核对所有 `pub(super)` 是否已补齐
- `cargo test --workspace`
- 单独 commit

### 第 8 步：搬 D 组线程扩展操作到 `codex_threads.rs`

- 搬 `MAX_SORT_KEY_BATCH`
- 搬 `SQLiteStorageAdapter::{find_archived_thread_by_title, move_codex_thread_workspace, codex_thread_sort_key, codex_thread_sort_keys}`
- 搬线程时间戳与 rollout fallback helper
- `cargo test --workspace`
- 单独 commit

### 第 9 步：整理 `mod.rs` 只保留 re-export 与跨组测试

- 删除 `mod.rs` 中已经迁出的运行时代码
- 保留扁平 `pub use`
- 把跨组集成测试留在 `mod.rs`
- 把 D 组纯 helper 测试迁到 `codex_threads.rs`
- `cargo test --workspace`
- 单独 commit

### 第 10 步：最终核对

- 再跑一次 `cargo test --workspace`
- `rg -n "pub\\(crate\\) fn|pub\\(crate\\) enum|pub\\(crate\\) const" crates/codex-pilot-data/src/storage`
  - 核对没有遗漏的过宽可见性
- `rg -n "mod tests|#\\[test\\]" crates/codex-pilot-data/src/storage`
  - 核对测试是否按本报告归位
- 检查 `codex_pilot_data::storage::*` 对外路径是否完全不变

## 8. 供 T20b 直接照抄的最小规则

- 不改 `lib.rs`
- 不改公开 API 名称和对外路径
- 不新增超出本报告范围的 helper
- 不额外升别的 `pub(super)`；只升第 5 节列出的项
- 不把跨组集成测试强拆到单模块
- 每一步独立 commit，每一步跑 `cargo test --workspace`
