# `markdown.rs` 拆分映射报告

更新时间：2026-05-27  
目标文件：[`crates/codex-pilot-data/src/markdown.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/markdown.rs)  
当前文件长度：1118 行

## 0. 范围与结论

本报告只做拆分调研与映射，不修改 [`markdown.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/markdown.rs) 任何代码，也不新建任何 `.rs` 文件。

已核对的现状：

- 对外调用面只有 2 处，均为 [`routes_sessions.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs:84) / [`routes_sessions.rs`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs:97) 中的 `MarkdownExportService::new(...)`
- 当前公开面只有 3 个：`ExportStatus`、`ExportResult`、`MarkdownExportService`
- `markdown.rs` 反向依赖 `storage` 的 5 项：`SchemaKind`、`SessionRef`、`has_columns`、`normalize_session_id`、`schema_kind`
- 文件内另有 2 个核心内部类型：`Message`、`MessageBlock`
- 当前文件有 `#[cfg(test)] mod tests`
- 当前文件没有顶层 `const` / `static`

结论先行：

- 这次拆分不需要照搬 `storage.rs` 的多层底座结构，建议拆成 4 个子文件加 1 个 `mod.rs`
- `storage` 的 5 项 helper 继续从 `crate::storage::{...}` 导入，不建议为了 `markdown` 拆分反向改 `storage`
- 渲染逻辑可以独立成纯函数子模块，但 `render_html` 仍需保留 `exported_at_label()` 这一薄层时间读取入口
- `format` helper 暂不建议升到 `pub(crate)`；先按 `markdown` 私有收口，未来真有第二个消费者再提升

## 1. 目标文件结构

建议目标结构：

```text
crates/codex-pilot-data/src/markdown/
├── mod.rs
├── models.rs
├── export.rs
├── render.rs
└── format.rs
```

各文件职责建议如下：

- `mod.rs`
  - `mod` 声明
  - `pub use`：`ExportStatus`、`ExportResult`、`MarkdownExportService`
  - `impl MarkdownExportService`
  - 少量跨文件 glue：`ExportFormat`
  - 保留跨流程测试
- `models.rs`
  - `Message`
  - `MessageBlock`
  - `ExportStatus`
  - `ExportResult`
  - `exported` / `not_found` / `failed`
- `export.rs`
  - `export_generic_session`
  - `export_codex_thread`
  - `fetch_optional_title`
  - `fetch_generic_messages`
  - `load_rollout_messages`
  - `serialize_message_content`
  - `display_role`
  - `display_title`
  - `build_filename`
- `render.rs`
  - `render_markdown`
  - `render_html`
  - `render_markdown_body`
  - `render_html_body`
  - `render_html_text`
  - `render_image_block`
  - `role_class`
  - `display_speaker_label`
  - `avatar_markup`
  - `robot_icon` / `user_icon` / `system_icon` / `image_icon`
  - `display_message_time`
  - `escape_html`
- `format.rs`
  - `format_unix_utc`
  - `civil_from_days`
  - `text_blocks`
  - `strip_image_tags`
  - `extract_image_src`
  - `TextPart`
  - `split_fenced_code`
  - `parse_time_hhmm`
  - `normalize_newlines`
  - `replace_filename_chars`
  - `exported_at_label`

这样分组比“入口编排 / 数据模型 / 渲染 / format helper”的直觉方案只多了一个细化点：把 `display_role` / `display_title` / `build_filename` 放进 `export.rs`，而不是扔给 `format.rs`。理由是它们不是通用格式化工具，而是导出流程语义的一部分：

- `display_role` 负责把存储层 role 映射成导出语义
- `display_title` 负责导出标题的兜底语义
- `build_filename` 负责导出产物命名，而不是通用字符串清洗

## 2. 顶层项归属表

说明：

- 只列当前 `markdown.rs` 顶层 `fn` / `struct` / `enum` / `const`
- 不含 `impl MarkdownExportService` 里的方法
- 不把测试模块内函数计入“顶层项”
- `callers` 以当前文件内实际调用关系为主；外部调用单独点明
- 当前文件没有顶层 `const` / `static`

| 名称 | 当前行号 | 当前可见性 | callers | 建议归属 | 建议可见性 | 备注 |
| --- | ---: | --- | --- | --- | --- | --- |
| `ExportStatus` | 12 | `pub enum` | `ExportResult`、`exported`、`not_found`、`failed`、测试断言 | `models.rs` | `pub` | 现有公开 API，保持不变。 |
| `ExportResult` | 19 | `pub struct` | `MarkdownExportService::{export, export_markdown, export_html}` 返回值；`exported`、`not_found`、`failed` | `models.rs` | `pub` | 现有公开 API，保持不变。 |
| `MarkdownExportService` | 29 | `pub struct` | 外部：[`routes_sessions.rs:84`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs:84)、[`routes_sessions.rs:97`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs:97)；内部测试 | `mod.rs` | `pub` | 公开入口，不建议挪到子模块后再改导出路径。 |
| `Message` | 34 | `struct` | `fetch_generic_messages`、`load_rollout_messages`、`render_markdown`、`render_html`、`exported` | `models.rs` | `pub(super)` | 拆分后会被 `export.rs`、`render.rs`、`mod.rs` 共享。 |
| `MessageBlock` | 41 | `enum` | `Message`、`serialize_message_content`、`text_blocks`、`render_*_body` | `models.rs` | `pub(super)` | 和 `Message` 同层最稳。 |
| `ExportFormat` | 88 | `enum` | `MarkdownExportService::export_markdown`、`export_html`、`export_with`、`export_generic_session`、`export_codex_thread`、`exported` | `mod.rs` | `pub(super)` | 只服务模块内部流程控制。 |
| `export_generic_session` | 93 | `fn` | `MarkdownExportService::export_with` | `export.rs` | `pub(super)` | `mod.rs` 中的 `export_with` 需要调用。 |
| `export_codex_thread` | 107 | `fn` | `MarkdownExportService::export_with` | `export.rs` | `pub(super)` | 同上。 |
| `fetch_optional_title` | 139 | `fn` | `export_generic_session` | `export.rs` | `fn` | 只服务 generic session 分支。 |
| `fetch_generic_messages` | 164 | `fn` | `export_generic_session` | `export.rs` | `fn` | 只服务 generic session 分支。 |
| `load_rollout_messages` | 225 | `fn` | `export_codex_thread` | `export.rs` | `fn` | 只服务 codex thread 分支。 |
| `serialize_message_content` | 259 | `fn` | `load_rollout_messages` | `export.rs` | `fn` | rollout JSONL 解析细节。 |
| `render_markdown` | 280 | `fn` | `exported` | `render.rs` | `pub(super)` | `models.rs` 中的 `exported` 会调用。 |
| `render_html` | 298 | `fn` | `exported` | `render.rs` | `pub(super)` | 同上。 |
| `exported_at_label` | 530 | `fn` | `render_html` | `format.rs` | `pub(super)` | 只被 HTML 渲染调用，但属于时间格式化入口。 |
| `format_unix_utc` | 537 | `fn` | `exported_at_label`、测试 `formats_unix_time_as_readable_utc` | `format.rs` | `pub(super)` | 若保留现有单测下沉，需对子模块测试可见。 |
| `civil_from_days` | 546 | `fn` | `format_unix_utc` | `format.rs` | `fn` | 纯内部 helper。 |
| `text_blocks` | 563 | `fn` | `fetch_generic_messages`、`serialize_message_content` | `format.rs` | `pub(super)` | `export.rs` 需要调用。 |
| `strip_image_tags` | 572 | `fn` | `render_html_text` | `format.rs` | `pub(super)` | `render.rs` 需要调用。 |
| `extract_image_src` | 587 | `fn` | `serialize_message_content` | `format.rs` | `pub(super)` | `export.rs` 需要调用。 |
| `render_markdown_body` | 608 | `fn` | `render_markdown` | `render.rs` | `fn` | 只服务 markdown 渲染。 |
| `render_html_body` | 620 | `fn` | `render_html` | `render.rs` | `fn` | 只服务 HTML 渲染。 |
| `render_html_text` | 631 | `fn` | `render_html_body` | `render.rs` | `fn` | 只服务 HTML 渲染。 |
| `TextPart` | 647 | `enum` | `render_html_text`、`split_fenced_code` | `format.rs` | `pub(super)` | `render.rs` 消费 `split_fenced_code` 返回值。 |
| `split_fenced_code` | 652 | `fn` | `render_html_text` | `format.rs` | `pub(super)` | `render.rs` 需要调用。 |
| `render_image_block` | 690 | `fn` | `render_html_body` | `render.rs` | `fn` | 只服务 HTML 渲染。 |
| `role_class` | 703 | `fn` | `render_html` | `render.rs` | `fn` | 只服务 HTML 样式映射。 |
| `display_speaker_label` | 711 | `fn` | `render_html` | `render.rs` | `fn` | 只服务渲染 label。 |
| `avatar_markup` | 719 | `fn` | `render_html` | `render.rs` | `fn` | 只服务 HTML 渲染。 |
| `robot_icon` | 727 | `fn` | `avatar_markup` | `render.rs` | `fn` | 只服务图标拼装。 |
| `user_icon` | 731 | `fn` | `avatar_markup` | `render.rs` | `fn` | 同上。 |
| `system_icon` | 735 | `fn` | `avatar_markup` | `render.rs` | `fn` | 同上。 |
| `image_icon` | 739 | `fn` | `render_image_block` | `render.rs` | `fn` | 同上。 |
| `display_message_time` | 743 | `fn` | `render_html` | `render.rs` | `fn` | 虽依赖 `parse_time_hhmm`，语义仍是渲染层。 |
| `parse_time_hhmm` | 747 | `fn` | `display_message_time` | `format.rs` | `pub(super)` | 渲染层依赖的纯文本时间解析。 |
| `escape_html` | 762 | `fn` | `render_html`、`render_html_text`、`render_image_block` | `render.rs` | `fn` | 目前只被渲染层使用，不必外提。 |
| `exported` | 777 | `fn` | `export_generic_session`、`export_codex_thread` | `models.rs` | `pub(super)` | `export.rs` 会构造 `ExportResult`，需跨文件可见。 |
| `not_found` | 803 | `fn` | `export_generic_session`、`export_codex_thread` | `models.rs` | `pub(super)` | 同上。 |
| `failed` | 814 | `fn` | `MarkdownExportService::export_with`、`export_codex_thread` | `models.rs` | `pub(super)` | `mod.rs` 与 `export.rs` 都需要。 |
| `display_role` | 825 | `fn` | `fetch_generic_messages`、`load_rollout_messages` | `export.rs` | `fn` | 导出语义映射，不是通用 helper。 |
| `display_title` | 835 | `fn` | `export_generic_session`、`export_codex_thread`、`display_role` | `export.rs` | `fn` | 标题兜底语义留在导出层。 |
| `build_filename` | 847 | `fn` | `exported` | `export.rs` | `pub(super)` | `models.rs` 的 `exported` 需要调用。 |
| `normalize_newlines` | 868 | `fn` | `text_blocks`、`split_fenced_code`、`display_title` | `format.rs` | `pub(super)` | `export.rs` 与 `format.rs` 都会用。 |
| `replace_filename_chars` | 872 | `fn` | `build_filename` | `format.rs` | `pub(super)` | `export.rs` 通过 `build_filename` 间接依赖。 |

补充：

- `impl MarkdownExportService` 内部方法 `new` / `export` / `export_markdown` / `export_html` / `export_with` 不在本表范围内
- `rg` 命中了测试字符串里的 `fn main() {}`，那不是实际顶层项，不计入本表

## 3. 可见性升级清单

这一节只列“为了按第 1 节的文件结构拆分后还能编译”必须提升到 `pub(super)` 的项。

### 3.1 需要升到 `pub(super)` 的项

- 类型
  - `Message`
  - `MessageBlock`
  - `ExportFormat`
  - `TextPart`
- 导出编排入口
  - `export_generic_session`
  - `export_codex_thread`
- 结果构造
  - `exported`
  - `not_found`
  - `failed`
- 渲染入口
  - `render_markdown`
  - `render_html`
- 被跨子模块调用的格式化 / 解析 helper
  - `exported_at_label`
  - `format_unix_utc`
  - `text_blocks`
  - `strip_image_tags`
  - `extract_image_src`
  - `split_fenced_code`
  - `parse_time_hhmm`
  - `normalize_newlines`
  - `replace_filename_chars`
- 被 `models.rs` / `export.rs` 交叉依赖的导出 helper
  - `build_filename`

### 3.2 可以保持私有的项

- `fetch_optional_title`
- `fetch_generic_messages`
- `load_rollout_messages`
- `serialize_message_content`
- `civil_from_days`
- `render_markdown_body`
- `render_html_body`
- `render_html_text`
- `render_image_block`
- `role_class`
- `display_speaker_label`
- `avatar_markup`
- `robot_icon`
- `user_icon`
- `system_icon`
- `image_icon`
- `display_message_time`
- `escape_html`
- `display_role`
- `display_title`

### 3.3 不应该升级的清单

这组明确不建议为了“可能以后会用”就升到 `pub(crate)` 或继续外放：

- `Message` / `MessageBlock` 不应升到 `pub(crate)`；它们只该是 `markdown` 模块内部协议
- `TextPart` 不应升到 `pub(crate)`；它只是 fenced code 拆段细节
- `render_markdown_body` / `render_html_body` / `render_html_text` 不应升级；这些都是具体渲染实现
- `role_class` / `display_speaker_label` / `avatar_markup` / `robot_icon` / `user_icon` / `system_icon` / `image_icon` 不应升级；这是当前 HTML 模板细节
- `escape_html` 不应先行提升成通用 crate 级工具；当前只有 `markdown` 渲染使用
- `display_role` / `display_title` / `build_filename` 不应提升到 `pub(crate)`；它们带有导出语义，不应伪装成通用工具
- `format_unix_utc`、`normalize_newlines`、`replace_filename_chars` 暂不建议升到 `pub(crate)`；除非未来出现第二个真实消费者

## 4. 测试归属表

当前文件存在 `#[cfg(test)] mod tests`，起始于 [`markdown.rs:884`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-data/src/markdown.rs:884)。

建议原则：

- 直接覆盖公开入口、跨越 DB/rollout/渲染的测试，继续放 `mod.rs`
- 纯格式化 helper 的单测，可以跟着下沉到对应子文件
- 第一轮实施不建议额外新建 `test_support.rs`

| 测试 / fixture | 当前行号 | 覆盖范围 | 建议归属 |
| --- | ---: | --- | --- |
| `unique_temp_path` | 890 | 测试临时路径 fixture | `mod.rs` |
| `exports_generic_session_markdown` | 902 | generic session 导出主流程 | `mod.rs` |
| `exports_codex_rollout_markdown` | 931 | codex thread 导出主流程 | `mod.rs` |
| `exports_html_with_escaped_content` | 965 | HTML 导出主流程 + 转义 + fenced code | `mod.rs` |
| `exports_rollout_html_with_clean_image_attachment` | 1003 | rollout image + HTML 渲染 | `mod.rs` |
| `exports_rollout_html_image_sources_and_placeholder` | 1042 | image source 解析 + placeholder HTML | `mod.rs` |
| `markdown_export_keeps_image_wrappers_and_uses_safe_image_placeholder` | 1078 | Markdown 渲染与 image wrapper 语义 | `mod.rs` |
| `formats_unix_time_as_readable_utc` | 1114 | `format_unix_utc` | `format.rs` |

说明：

- 当前没有只测 `split_fenced_code`、`normalize_newlines`、`display_title` 之类的纯 helper 单测；实施时不要顺手补新测试并改行为
- 如果实施第一步只做机械搬运，也可以临时把全部测试先留在 `mod.rs`，等结构稳定后再把 `formats_unix_time_as_readable_utc` 下沉到 `format.rs`

## 5. 风险点

### 5.1 `storage` 的 5 项 helper 还要不要继续从 `crate::storage::{...}` 走

建议继续走，理由如下：

- 当前 `markdown.rs` 只是 `storage` 的消费者，不是 `storage` 的拥有者
- 这 5 项正是 T20a §8.1 已确认需要保留 `pub(crate)` 的交叉点
- 为了 `markdown` 拆分去反向改 `storage` 的导出边界，会把单一任务变成跨任务重构，违反“修一个 bug / 一次任务不要顺手改别的”

因此拆分后仍建议保持：

```rust
use crate::storage::{SchemaKind, SessionRef, has_columns, normalize_session_id, schema_kind};
```

以及 `fetch_generic_messages` 中继续显式调用 `crate::storage::has_table(...)`。不建议顺手把 `has_table` 也加入 `use` 列表并修改代码风格。

### 5.2 HTML / Markdown 渲染能否独立成纯函数子模块

结论：基本可以，但要区分“主渲染入口”和“时间标签生成”。

- `render_markdown(title, &[Message]) -> String` 已经是纯函数
- `render_html(title, &[Message]) -> String` 只有一处外部状态依赖：`exported_at_label()` 读取系统时间
- 如果实施时坚持“不动业务逻辑”，那就让 `render_html` 继续调用 `exported_at_label()`，接受它是“近似纯函数 + 时间薄依赖”
- 如果未来另起优化任务，才考虑把 `exported_at` 作为参数注入，但这不属于本次拆分

所以第一轮拆分可以把 `render.rs` 视为“渲染为主、仅带一个系统时间入口”的子模块，而不是强求完全纯函数化。

### 5.3 `format` helper 未来会不会被别的模块复用

有复用潜力，但现在证据不足，不建议预先升到 `pub(crate)`。

最有可能被别处复用的是：

- `format_unix_utc`
- `normalize_newlines`
- `replace_filename_chars`

但当前都只有 `markdown` 一个消费者。按项目最小规则，第一轮实施应先收敛成 `markdown` 私有实现：

- 子模块间共享用 `pub(super)`
- 不额外升级到 `pub(crate)`
- 未来若出现第二个真实调用方，再单独开任务提升可见性并补测试

### 5.4 用 grep 验证是否还有遗漏调用方

已用 `rg` 核对：

- `MarkdownExportService::new` 只在 [`routes_sessions.rs:84`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs:84) 和 [`routes_sessions.rs:97`](/Users/huanglin/code/github/CodexPilot/crates/codex-pilot-core/src/routes_sessions.rs:97) 两处被生产代码调用
- `ExportStatus` / `ExportResult` / `MarkdownExportService` 在仓库中的其余命中均为本文件自身或测试

当前没有发现你给出的“外部只有 2 处 caller”存在遗漏。

## 6. 执行计划

参照 `storage` 的 10 步法，`markdown.rs` 建议压缩成 5 步。每一步单独 commit，且每一步都跑 `cargo test --workspace`，必须全绿。

1. 第 1 步：建立模块骨架  
   把 `markdown.rs` 改成 `markdown/mod.rs` 形态，先只搬 `mod` 声明、`pub use`、`MarkdownExportService`、`ExportFormat` 和原测试模块；不改变任何逻辑。

2. 第 2 步：搬 `models.rs`  
   搬 `ExportStatus`、`ExportResult`、`Message`、`MessageBlock`、`exported` / `not_found` / `failed`，按本报告把必要项升到 `pub(super)`。

3. 第 3 步：搬 `format.rs`  
   搬纯格式化和解析 helper：`format_unix_utc`、`civil_from_days`、`text_blocks`、`strip_image_tags`、`extract_image_src`、`TextPart`、`split_fenced_code`、`parse_time_hhmm`、`normalize_newlines`、`replace_filename_chars`、`exported_at_label`。

4. 第 4 步：搬 `render.rs`  
   搬 HTML/Markdown 渲染链路：`render_markdown`、`render_html`、`render_*_body`、图标和 speaker helper。此步结束后，渲染相关测试应继续全绿。

5. 第 5 步：搬 `export.rs` 并收口 imports  
   搬 `export_generic_session`、`export_codex_thread`、`fetch_*`、`load_rollout_messages`、`serialize_message_content`、`display_role`、`display_title`、`build_filename`；最后清理 `mod.rs` 的导入与 `pub(super)` 边界。

备注：

- 每一步都只做机械搬运和最小可见性调整
- 如果某一步需要为了编译通过而临时保留测试在 `mod.rs`，可以接受
- 不建议把“测试下沉整理”单独拆成第 6 步，除非第 5 步后文件边界仍不清楚

## 7. 最小规则

实施时建议直接照下面的最小规则执行：

- 不动业务逻辑
- 不改公开 API
- 不改 `MarkdownExportService::new` / `export` / `export_markdown` / `export_html` 的行为与签名
- 不改 `storage` 子模块，不新增对 `storage` 的反向依赖
- 不重命名变量，不重写 SQL，不调整 HTML/CSS 文案
- 不把 helper 先行抽成 crate 级通用工具
- 不“顺手”补新功能、不补新样式、不补新测试语义
- 除非编译所必需，不调整现有测试断言

## 8. 结论摘要

`markdown.rs` 适合按 `models / export / render / format` 四层拆开，复杂度明显低于 `storage.rs`。真正需要提升到 `pub(super)` 的项不多，主要集中在 `Message`、`MessageBlock`、`ExportFormat`、结果构造函数，以及少数被跨子模块调用的 format helper。只要坚持“先机械搬运、后再谈复用”，这次实施可以控制在 5 个 commit 内完成，不需要引入 `lib.rs` 或 `storage` 的额外改动。
