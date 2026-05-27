# `provider_sync.rs` 拆分映射报告

更新时间：2026-05-27  
目标文件：[`crates/codex-pilot-data/src/provider_sync.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/provider_sync.rs)  
当前文件长度：1106 行

## 0. 范围与结论

本报告只做拆分调研与映射，不修改 [`provider_sync.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/provider_sync.rs) 任何代码，也不新建任何 `.rs` 文件。

已核对的现状：

- 当前公开面 8 个：
  - `ProviderSyncStatus`
  - `ProviderSyncResult`
  - `ProviderCount`
  - `ProviderSyncInspection`
  - `inspect_provider_sync`
  - `inspect_provider_sync_with_target`
  - `run_provider_sync`
  - `run_provider_sync_with_target`
- 当前文件含 4 个文件级常量：
  - `DEFAULT_PROVIDER`
  - `SESSION_DIRS`
  - `BACKUP_KEEP_COUNT`
  - `MANAGED_BY`
- 当前文件含 2 个内部 struct：
  - `SessionChange`
  - `ProviderDriftDetail`
- 当前文件存在 `#[cfg(test)] mod tests`
- 当前文件直接做 rollout 文件遍历、锁目录创建/删除、SQLite 读取/更新、备份目录写入、诊断日志写入、全局状态 JSON 读写
- 当前 [`crates/codex-pilot-data/src/lib.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/lib.rs:1) 仍是 `pub mod provider_sync;`，因此后续拆分必须通过 `provider_sync/mod.rs` 的 `pub use` 保持对外路径不变
- 以 `inspect_provider_sync(_with_target)` / `run_provider_sync(_with_target)` / `ProviderSyncStatus::Synced` / `ProviderCount` 为口径，外部 caller 仍然集中在用户列的 5 个文件 8 处，没有新增第 6 个文件
- 另有 1 处额外公开类型引用未计入上面的“8 处”：[`apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:5`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:5) 的 `provider_sync_message(sync: ProviderSyncResult)` 签名

和设计文档的一致性说明：

- 已检查 [`docs/superpowers/specs/2026-05-20-provider-channel-simplification-design.md`](/Users/huanglin/code/github/CodexPilot/docs/superpowers/specs/2026-05-20-provider-channel-simplification-design.md:85) 中的 Provider Sync 设计约束。
- 本次只是拆分调研，不改任何业务行为；建议方案保持“Provider Sync 必须显式运行、保留备份、记录诊断日志、不要泄露会话内容”的现有设计口径，不需要改设计文档。

结论先行：

- 建议拆成 6 个子文件加 1 个 `mod.rs`，而不是把“共享 helper”和“IO 层”硬塞成两个过宽的大文件。
- `inspect` 与 `run` 共享的核心不是“普通字符串 helper”，而是 `SessionChange` 采集管线，所以应单独抽出 `session_changes.rs`，避免 inspect/run 再次双向粘连。
- 文件锁不是 inspect/run 共用逻辑，而是 run 独占的执行保护；锁 helper 应留在 `run.rs` 或它紧邻的执行层，不应沉到通用 IO 底座。
- SQLite 相关 helper 已经形成一个自然子群，建议从“IO 层”中单独析出 `sqlite.rs`，否则 IO 文件会同时承担 rollout 遍历、备份、SQLite、global state、diagnostic log 五类职责，仍然会偏大。

## 1. 目标文件结构

建议目标结构：

```text
crates/codex-pilot-data/src/provider_sync/
├── mod.rs
├── models.rs
├── inspect.rs
├── run.rs
├── session_changes.rs
├── sqlite.rs
└── filesystem.rs
```

各文件职责建议如下：

- `mod.rs`
  - `mod` 声明
  - 扁平 `pub use`
  - 对外 8 个公开面重导出，保持 `codex_pilot_data::provider_sync::<name>` 路径不变
- `models.rs`
  - `ProviderSyncStatus`
  - `ProviderSyncResult`
  - `ProviderCount`
  - `ProviderSyncInspection`
  - `SessionChange`
  - `ProviderDriftDetail`
  - `result(...)`
- `inspect.rs`
  - `inspect_provider_sync`
  - `inspect_provider_sync_with_target`
  - inspect 编排专属 glue
- `run.rs`
  - `run_provider_sync`
  - `run_provider_sync_with_target`
  - `acquire_lock`
  - `release_lock`
  - `schedule_provider_sync_delayed_recheck`
- `session_changes.rs`
  - rollout 文件遍历到 `SessionChange` 的采集管线
  - `collect_session_changes`
  - `rollout_files`
  - `collect_rollout_files`
  - `split_first_line`
  - `rollout_provider_from_first_line`
  - `rollout_provider_from_path`
- `sqlite.rs`
  - SQLite inspection / update / drift detail helper
  - `table_columns`
  - `count_sqlite_updates`
  - `count_sqlite_rows`
  - `count_sqlite_provider_rows_needing_sync`
  - `sqlite_provider_counts`
  - `sqlite_provider_drift_details`
  - `apply_sqlite_update`
- `filesystem.rs`
  - 非 SQLite 的共享文件 / 路径 / 诊断 / 备份 helper
  - `normalize_target_provider`
  - `dirs_home`
  - `read_current_provider`
  - `to_desktop_workspace_path`
  - `create_backup`
  - `apply_session_changes`
  - `restore_session_changes`
  - `load_global_state`
  - `normalized_global_state`
  - `count_global_state_updates`
  - `apply_global_state_update`
  - `path_array`
  - `dedupe_paths`
  - `log_provider_sync_event`
  - `diagnostic_log_path`
  - `prune_backups`
  - `timestamp_name`
  - `now_secs`
  - `now_ms`

为什么不采用“公开模型 / inspect / run / IO 层 / 共享 helper”五分法原样落地：

- `collect_session_changes` 既不是简单 IO，也不是普通共享 helper，它定义了 inspect/run 共同依赖的“变更收集协议”。如果把它塞进 `IO 层`，那个文件会变成新的中心节点。
- SQLite helper 的体量和耦合度都足够独立；把它们继续留在一个大而全的 IO 文件里，拆完之后最大的文件仍会是 IO，而不是编排入口。
- 文件锁与 delayed recheck 都只服务 run 生命周期，抽到共享 IO 反而会模糊“谁负责执行语义”。

如果后续实施更保守，也可以把 `session_changes.rs` 与 `filesystem.rs` 合并成一个 `io.rs`，得到 5 个子文件。但从当前 41 个函数的聚类看，6 文件结构更稳，不容易在第二轮再拆。

## 2. 顶层项归属表

说明：

- 只列当前 `provider_sync.rs` 顶层 `fn` / `struct` / `enum` / `const`
- 不把测试模块内函数计入本表
- `callers` 以当前文件内实际调用关系为主；外部 caller 在公开面项中单独点明

| 名称 | 当前行号 | 当前可见性 | callers | 建议归属 | 建议可见性 | 备注 |
| --- | ---: | --- | --- | --- | --- | --- |
| `DEFAULT_PROVIDER` | 10 | `const` | `normalize_target_provider`、`read_current_provider` | `filesystem.rs` | `const` | 默认 provider 语义只服务配置/路径侧 helper。 |
| `SESSION_DIRS` | 11 | `const` | `rollout_files` | `session_changes.rs` | `const` | 只服务 rollout 目录扫描。 |
| `BACKUP_KEEP_COUNT` | 12 | `const` | `prune_backups` | `filesystem.rs` | `const` | 只服务备份保留策略。 |
| `MANAGED_BY` | 13 | `const` | `create_backup`、`prune_backups` | `filesystem.rs` | `const` | 备份 metadata 协议常量。 |
| `ProviderSyncStatus` | 17 | `pub enum` | `ProviderSyncResult`、`result`；外部：[`launch_helpers.rs:160`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs:160) | `models.rs` | `pub` | 公开 API，路径保持不变。 |
| `ProviderSyncResult` | 23 | `pub struct` | `run_provider_sync`、`run_provider_sync_with_target`、`result`；外部：[`provider.rs:5`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:5) | `models.rs` | `pub` | 公开 API，路径保持不变。 |
| `ProviderCount` | 33 | `pub struct` | `ProviderSyncInspection`、`provider_counts`；外部：[`lib.rs:220`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/lib.rs:220)、[`provider_store_types.rs:56`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/provider_store_types.rs:56)、[`provider_store_types.rs:57`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/provider_store_types.rs:57) | `models.rs` | `pub` | 公开 API，路径保持不变。 |
| `ProviderSyncInspection` | 39 | `pub struct` | `inspect_provider_sync`、`inspect_provider_sync_with_target`；外部通过 inspect 返回值使用 | `models.rs` | `pub` | 公开 API，路径保持不变。 |
| `SessionChange` | 51 | `struct` | `collect_session_changes`、`create_backup`、`apply_session_changes`、`restore_session_changes`、inspect/run 编排 | `models.rs` | `pub(super)` | inspect/run/session_changes/filesystem 四方共享，必须升级。 |
| `ProviderDriftDetail` | 63 | `struct` | `sqlite_provider_drift_details`、`log_provider_sync_event` | `models.rs` | `pub(super)` | 只供模块内部诊断 JSON 使用，不应外放到 crate 级。 |
| `inspect_provider_sync` | 74 | `pub fn` | 外部：inspect 调用入口 | `inspect.rs` | `pub` | 公开 API，路径保持不变。 |
| `inspect_provider_sync_with_target` | 81 | `pub fn` | `inspect_provider_sync`；外部：[`provider.rs:469`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:469)、[`launch_helpers.rs:67`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs:67)、[`diagnostics.rs:199`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/diagnostics.rs:199) | `inspect.rs` | `pub` | 公开 API，路径保持不变。 |
| `run_provider_sync` | 133 | `pub fn` | 外部 run 入口包装 | `run.rs` | `pub` | 公开 API，路径保持不变。 |
| `run_provider_sync_with_target` | 141 | `pub fn` | `run_provider_sync`；外部：[`provider.rs:513`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:513)、[`launch_helpers.rs:125`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs:125) | `run.rs` | `pub` | 公开 API，路径保持不变。 |
| `normalize_target_provider` | 283 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | inspect/run 共用的轻量输入归一化。 |
| `result` | 292 | `fn` | `run_provider_sync_with_target` | `models.rs` | `pub(super)` | run 编排跨文件构造 `ProviderSyncResult` 时需要。 |
| `dirs_home` | 310 | `fn` | `inspect_provider_sync`、`run_provider_sync`、`diagnostic_log_path` | `filesystem.rs` | `pub(super)` | 路径 helper，不建议散在 inspect/run。 |
| `read_current_provider` | 317 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync`、`run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | 配置读取 helper。 |
| `acquire_lock` | 341 | `fn` | `run_provider_sync_with_target` | `run.rs` | `fn` | run 独占，不建议公共化。 |
| `release_lock` | 350 | `fn` | `run_provider_sync_with_target` | `run.rs` | `fn` | 同上。 |
| `collect_session_changes` | 357 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync_with_target` | `session_changes.rs` | `pub(super)` | inspect/run 共用核心管线。 |
| `rollout_provider_from_first_line` | 408 | `fn` | `inspect_provider_sync_with_target`、`rollout_provider_from_path` | `session_changes.rs` | `pub(super)` | inspect 统计和 drift detail 都需要。 |
| `provider_counts` | 418 | `fn` | `inspect_provider_sync_with_target` | `models.rs` | `pub(super)` | 输出 `ProviderCount`，放模型同层更自然。 |
| `rollout_files` | 436 | `fn` | `collect_session_changes` | `session_changes.rs` | `fn` | 只服务变更采集。 |
| `collect_rollout_files` | 448 | `fn` | `rollout_files` | `session_changes.rs` | `fn` | 只服务 rollout 递归遍历。 |
| `split_first_line` | 464 | `fn` | `collect_session_changes` | `session_changes.rs` | `fn` | 只服务 rollout 首行解析。 |
| `to_desktop_workspace_path` | 472 | `fn` | `collect_session_changes`、`normalized_global_state`、`dedupe_paths` | `filesystem.rs` | `pub(super)` | 变更采集与 global state 共用。 |
| `create_backup` | 487 | `fn` | `run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | run 入口调用，但本质是备份协议。 |
| `apply_session_changes` | 540 | `fn` | `run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | 回写 rollout 文件动作，run 需要。 |
| `restore_session_changes` | 550 | `fn` | `run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | run 失败回滚动作。 |
| `table_columns` | 560 | `fn` | `count_sqlite_updates`、`count_sqlite_rows`、`count_sqlite_provider_rows_needing_sync`、`sqlite_provider_counts`、`sqlite_provider_drift_details`、`apply_sqlite_update` | `sqlite.rs` | `fn` | 纯 SQLite 内部 helper。 |
| `count_sqlite_updates` | 570 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync_with_target` | `sqlite.rs` | `pub(super)` | inspect/run 共用。 |
| `count_sqlite_rows` | 610 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync_with_target` 日志 | `sqlite.rs` | `pub(super)` | inspect/run 共用。 |
| `count_sqlite_provider_rows_needing_sync` | 623 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync_with_target`、`schedule_provider_sync_delayed_recheck` | `sqlite.rs` | `pub(super)` | inspect/run/delayed recheck 共用。 |
| `sqlite_provider_counts` | 642 | `fn` | `inspect_provider_sync_with_target`、`run_provider_sync_with_target`、`schedule_provider_sync_delayed_recheck` | `sqlite.rs` | `pub(super)` | 统计 helper。 |
| `sqlite_provider_drift_details` | 671 | `fn` | `run_provider_sync_with_target`、`schedule_provider_sync_delayed_recheck` | `sqlite.rs` | `pub(super)` | drift detail 只在 run 诊断链使用，但 delayed recheck 也要调。 |
| `rollout_provider_from_path` | 721 | `fn` | `sqlite_provider_drift_details` | `session_changes.rs` | `pub(super)` | drift detail 需要从 rollout_path 反查 provider。 |
| `schedule_provider_sync_delayed_recheck` | 727 | `fn` | `run_provider_sync_with_target` | `run.rs` | `fn` | run 独占；当前实现含 `std::thread::sleep`，拆分时只搬位置，不顺手改逻辑。 |
| `log_provider_sync_event` | 744 | `fn` | `run_provider_sync_with_target`、`schedule_provider_sync_delayed_recheck` | `filesystem.rs` | `pub(super)` | run 与 delayed recheck 共用。 |
| `diagnostic_log_path` | 759 | `fn` | `log_provider_sync_event` | `filesystem.rs` | `fn` | 只服务诊断日志写入。 |
| `apply_sqlite_update` | 772 | `fn` | `run_provider_sync_with_target` | `sqlite.rs` | `pub(super)` | run 编排需要。 |
| `load_global_state` | 811 | `fn` | `count_global_state_updates`、`apply_global_state_update` | `filesystem.rs` | `fn` | global state 内部 helper。 |
| `normalized_global_state` | 821 | `fn` | `count_global_state_updates`、`apply_global_state_update` | `filesystem.rs` | `fn` | global state 内部 helper。 |
| `count_global_state_updates` | 865 | `fn` | `run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | run 编排需要。 |
| `apply_global_state_update` | 874 | `fn` | `run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | run 编排需要。 |
| `path_array` | 890 | `fn` | `normalized_global_state` | `filesystem.rs` | `fn` | 纯 global state helper。 |
| `dedupe_paths` | 905 | `fn` | `normalized_global_state` | `filesystem.rs` | `fn` | 纯 global state helper。 |
| `prune_backups` | 923 | `fn` | `run_provider_sync_with_target` | `filesystem.rs` | `pub(super)` | run 编排需要。 |
| `timestamp_name` | 951 | `fn` | `create_backup` | `filesystem.rs` | `fn` | 备份命名 helper。 |
| `now_secs` | 955 | `fn` | `acquire_lock`、`timestamp_name`、测试 fixture | `filesystem.rs` | `pub(super)` | 如果测试继续留在 `mod.rs`，需要对子模块测试可见。 |
| `now_ms` | 962 | `fn` | `log_provider_sync_event` | `filesystem.rs` | `fn` | 只服务日志时间戳。 |

## 3. 可见性升级清单

目标原则：

- 对外公开面只保留现有 8 个，并继续通过 `codex_pilot_data::provider_sync::<name>` 访问
- 模块间共享默认用 `pub(super)`，不要越级抬成 `pub(crate)`
- 纯实现细节继续留私有

### 3.1 需要升到 `pub(super)` 的项

- 类型
  - `SessionChange`
  - `ProviderDriftDetail`
- 结果 / 统计构造
  - `result`
  - `provider_counts`
- inspect / run 共用 helper
  - `normalize_target_provider`
  - `dirs_home`
  - `read_current_provider`
  - `collect_session_changes`
  - `rollout_provider_from_first_line`
  - `to_desktop_workspace_path`
- run 与共享文件层交叉调用的 helper
  - `create_backup`
  - `apply_session_changes`
  - `restore_session_changes`
  - `log_provider_sync_event`
  - `count_global_state_updates`
  - `apply_global_state_update`
  - `prune_backups`
  - `now_secs`
- inspect/run 与 SQLite 子模块交叉调用的 helper
  - `count_sqlite_updates`
  - `count_sqlite_rows`
  - `count_sqlite_provider_rows_needing_sync`
  - `sqlite_provider_counts`
  - `sqlite_provider_drift_details`
  - `apply_sqlite_update`
- SQLite 与 rollout 采集层交叉调用的 helper
  - `rollout_provider_from_path`

### 3.2 可以保持私有的项

- `acquire_lock`
- `release_lock`
- `rollout_files`
- `collect_rollout_files`
- `split_first_line`
- `table_columns`
- `schedule_provider_sync_delayed_recheck`
- `diagnostic_log_path`
- `load_global_state`
- `normalized_global_state`
- `path_array`
- `dedupe_paths`
- `timestamp_name`
- `now_ms`

### 3.3 不应该升级的清单

这组显式列出来，避免机械拆分时“先抬高再说”：

- 不应把 `SessionChange` / `ProviderDriftDetail` 升到 `pub(crate)`
  - 它们只是 `provider_sync` 模块内部协议，不是 crate 级复用模型
- 不应把 `acquire_lock` / `release_lock` 升到 `pub(super)` 或 `pub(crate)`
  - 文件锁是 run 生命周期细节，不是 inspect/shared IO 合同
- 不应把 `rollout_files` / `collect_rollout_files` / `split_first_line` 升级
  - 它们只服务 `collect_session_changes`
- 不应把 `table_columns` 升级
  - 它没有跨模块调用需求，纯属 SQLite 实现细节
- 不应把 `diagnostic_log_path` / `path_array` / `dedupe_paths` / `timestamp_name` / `now_ms` 升级
  - 这些都是单文件内部 helper，不值得暴露
- 不应把任何 helper 进一步升到 `pub(crate)`
  - 当前 grep 没发现 `storage` / `markdown` / `core` / `manager` 直接依赖这些内部 helper；跨模块可见性只需父模块即可

### 3.4 对外 re-export 规则

拆分后 `provider_sync/mod.rs` 必须保留下面这类重导出，保证外部路径稳定：

- `pub use models::{ProviderCount, ProviderSyncInspection, ProviderSyncResult, ProviderSyncStatus};`
- `pub use inspect::{inspect_provider_sync, inspect_provider_sync_with_target};`
- `pub use run::{run_provider_sync, run_provider_sync_with_target};`

不应把内部 helper 从 `mod.rs` 继续 re-export。

## 4. 测试归属表

当前文件存在 `#[cfg(test)] mod tests`，起始于 [`provider_sync.rs:969`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/provider_sync.rs:969)。

当前测试只有 2 个测试函数和 1 个 fixture helper。因为两条测试都跨 rollout 文件、SQLite、global state、备份目录，所以不建议第一轮下沉到某个细分子模块；它们应继续留在 `provider_sync/mod.rs` 作为跨模块集成测试。

| 测试 / fixture | 当前行号 | 覆盖范围 | 建议归属 |
| --- | ---: | --- | --- |
| `provider_sync_updates_rollout_sqlite_and_global_state` | 974 | rollout 改写 + SQLite 更新 + global state 归一化 + 备份 manifest | `provider_sync/mod.rs` |
| `provider_sync_skips_when_lock_exists` | 1087 | lock 存在时的 run 跳过语义 | `provider_sync/mod.rs` |
| `unique_temp_dir` | 1103 | 测试临时目录 fixture | `provider_sync/mod.rs` |

补充说明：

- 当前没有纯 helper 单测，因此不需要像 `markdown.rs` 那样把某个测试下沉到 format 子模块。
- `unique_temp_dir` 依赖 `now_secs`；如果未来想把 fixture 移出 `mod.rs`，才需要重新评估 `now_secs` 的测试可见性。

## 5. 风险点

### 5.1 文件锁是 inspect/run 共用，还是 run 独占

结论：run 独占。

证据：

- `acquire_lock` 只被 [`run_provider_sync_with_target`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/provider_sync.rs:141) 调用
- `release_lock` 只被 [`run_provider_sync_with_target`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/provider_sync.rs:141) 调用
- inspect 路径完全没有锁目录参与

建议落点：

- 放 `run.rs`
- 保持私有 `fn`

理由：

- 锁是“执行保护”而不是“文件系统公共能力”
- 放到 `filesystem.rs` 会误导后续维护者，以为 inspect 也需要锁

### 5.2 inspect 和 run 是否共用同一套 `SessionChange` pipeline

结论：是，共用同一套，而且值得单独抽成 `session_changes.rs`。

证据：

- inspect 在一开始调用 `collect_session_changes(&home, &target_provider)?`
- run 在真正执行前也调用同一个 `collect_session_changes(&home, &target_provider)?`
- 两边都依赖它产出的：
  - `thread_ids_with_user_events`
  - `cwd_by_thread_id`
  - rollout provider 分布
  - `rewrite_needed` 计数

建议：

- 把 `SessionChange` 视为“变更收集协议”
- `collect_session_changes` 及其 rollout 遍历 helper 独立成子模块

不建议的做法：

- 不要把这套逻辑塞进 `inspect.rs`
  - 否则 run 会反向依赖 inspect
- 不要塞进笼统的 `io.rs`
  - 否则 IO 文件会重新成为中心文件

### 5.3 4 个常量的归属

建议：

- `DEFAULT_PROVIDER` -> `filesystem.rs`
- `SESSION_DIRS` -> `session_changes.rs`
- `BACKUP_KEEP_COUNT` -> `filesystem.rs`
- `MANAGED_BY` -> `filesystem.rs`

理由：

- 这 4 个常量并不共享同一语义面
- 其中只有 `SESSION_DIRS` 和 rollout 目录遍历强绑定，剩余 3 个都跟配置默认值/备份协议有关

不建议：

- 不要为了“集中管理常量”新建单独 `consts.rs`
  - 当前只有 4 个常量，单拉一层会徒增跳转
- 不要把 `SESSION_DIRS` 留在共享 helper 文件
  - 它只被 `rollout_files` 使用，跟其他 helper 无共享价值

### 5.4 用 grep 验证 8 处外部 caller 是否完整

按用户给定口径核对：

- `apps/codex-pilot-manager/src-tauri/src/commands/provider.rs`
  - [`provider.rs:469`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:469) `inspect_provider_sync_with_target`
  - [`provider.rs:513`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/provider.rs:513) `run_provider_sync_with_target`
  - `provider_sync_message(...)` 本身不是函数调用点，但其参数类型引用了 `ProviderSyncResult`
- `apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs`
  - [`launch_helpers.rs:67`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs:67) `inspect_provider_sync_with_target`
  - [`launch_helpers.rs:125`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs:125) `run_provider_sync_with_target`
  - [`launch_helpers.rs:160`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/launch_helpers.rs:160) `ProviderSyncStatus::Synced`
- `apps/codex-pilot-manager/src-tauri/src/lib.rs`
  - [`lib.rs:220`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/lib.rs:220) `ProviderCount`
- `apps/codex-pilot-manager/src-tauri/src/provider_store_types.rs`
  - [`provider_store_types.rs:56`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/provider_store_types.rs:56) `ProviderCount`
  - [`provider_store_types.rs:57`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/provider_store_types.rs:57) `ProviderCount`
- `apps/codex-pilot-manager/src-tauri/src/commands/diagnostics.rs`
  - [`diagnostics.rs:199`](/Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager/src-tauri/src/commands/diagnostics.rs:199) `inspect_provider_sync_with_target`

结论：

- 以用户给的“5 个文件 8 处”主口径看，没有遗漏文件，也没有第 9 个函数调用点
- 但若把公开类型引用一并算进来，`provider.rs:5` 的 `ProviderSyncResult` 还应补记为“额外 1 处类型签名引用”
- 当前 grep 也再次确认：没有来自 `storage` / `markdown` 的反向依赖

### 5.5 额外风险：run 编排与共享 helper 的边界

run 现在同时负责：

- 锁
- inspect 式预检查
- 诊断日志 before/after
- 备份
- rollout 改写
- SQLite 事务
- global state 更新
- 失败回滚
- delayed recheck

拆分时最容易出的问题不是业务逻辑，而是把这些 helper 错放到“共享层”后形成新的来回依赖。

建议边界：

- `run.rs` 只保留流程控制、锁、延迟复检调度
- 其余动作作为被调用方下沉
- 但不要再把 `result(...)`、`SessionChange` 定义、或 `log_provider_sync_event` 回挪到 `run.rs`

## 6. 执行计划

建议 5 步，每一步单独 commit，每一步后都跑 `cargo test --workspace`，必须全绿。

### 第 1 步：搭骨架，不搬逻辑

- 新建 `crates/codex-pilot-data/src/provider_sync/` 目录和 `mod.rs`
- 在 `mod.rs` 中声明目标子模块并先保留原文件内容逐步迁移
- 建好对外 `pub use` 骨架，确保最终公开路径保持不变

提交目标：

- 只有模块骨架和 re-export 准备
- 不改变任何业务逻辑

### 第 2 步：先搬模型与共享类型

- 把公开模型、`SessionChange`、`ProviderDriftDetail`、`result(...)`、`provider_counts(...)` 搬到 `models.rs`
- 补上第 3 节列出的最小 `pub(super)` 可见性
- 测试继续全部留在 `mod.rs`

提交目标：

- 消除“类型定义散落在入口文件中”的问题
- 不触及 rollout/SQLite 逻辑

### 第 3 步：抽 `session_changes.rs` 与 `filesystem.rs`

- 先搬 rollout 变更采集链
- 再搬配置读取、global state、备份、诊断日志、时间戳 helper
- 此步不要动 SQLite helper

提交目标：

- 让 inspect/run 脱离文件系统细节
- 保持 `collect_session_changes` 仍被原入口调用

### 第 4 步：抽 `sqlite.rs`

- 搬走 SQLite 读取、统计、drift detail、更新逻辑
- 只做机械搬运与 import 调整
- 不改 SQL，不改返回值，不改日志字段

提交目标：

- 让剩余入口文件只保留编排

### 第 5 步：收口 inspect/run 入口并整理测试落点

- 把 `inspect_provider_sync*` 收到 `inspect.rs`
- 把 `run_provider_sync*`、锁 helper、delayed recheck 收到 `run.rs`
- `mod.rs` 只保留 `pub use` 与测试
- 跑 `cargo test --workspace`

提交目标：

- 完成拆分
- 对外 API 路径不变
- 测试仍全部通过

## 7. 最小规则

实施时必须遵守下面这组最小规则：

- 不改业务逻辑
- 不改公开 API
- 不重命名现有变量、字段、事件名、SQL 字符串、备份文件名
- 不顺手修别的模块
- 不顺手改 `storage` 或 `markdown`
- 不顺手把 `std::thread::sleep` 改成异步 sleep
  - 这是现存问题，但不属于 T22 的拆分任务范围
- 不顺手把路径 helper 改成 `codex_pilot_core::app_paths`
  - 契约要求新行为遵循路径抽象；本任务不新增行为，只做机械拆分
- 不顺手改日志框架
  - 当前 `provider_sync.rs` 仍直接写诊断日志文件，这个行为本次只搬位置，不改实现
- 不顺手新增测试
  - 现有测试只做位置调整，不扩 scope
- 不顺手修改 `apps/codex-pilot-manager`、`crates/codex-pilot-core`、`crates/codex-pilot-data/src/lib.rs`
  - `lib.rs` 的公开模块声明保持原状；真正拆分时只在 `provider_sync` 子目录内部完成

## 8. 验收要点

T22 只是报告任务，验收标准是文档完整且自包含。后续 T22 实施阶段应额外检查：

- `codex_pilot_data::provider_sync::<name>` 的 8 个对外路径完全不变
- 外部 5 个文件的调用点不需要改 import 路径
- `provider_sync/mod.rs` 不重新长成第二个 500+ 行入口文件
- `SessionChange` 没被错误外放到 crate 级
- 文件锁仍只在 run 路径生效
- 现有两个集成测试继续通过
