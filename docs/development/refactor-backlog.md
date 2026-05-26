# CodexPilot 重构待办（Refactor Backlog）

本文件用于把当前已识别的代码质量问题落成**可让 Codex 独立执行**的任务清单。

> **使用方式**：每条任务自包含。把对应任务的"Codex Prompt"块**完整复制**到 Codex 里发出去，不要拼接、不要解释、不要让 Codex"看全局再决定"。一次只跑一条。完成后把状态从 `TODO` 改为 `DONE`，并在 PR / commit 里关联本任务编号。

> **维护者职责（你）**：只做设计 + 验收。不直接写实现代码。所有实现交给 Codex，按本文件顺序推进。

---

## 状态总览

| 编号  | 标题                              | 优先级 | 预估   | 状态 |
| ----- | --------------------------------- | ------ | ------ | ---- |
| T00   | 把硬性约束加进 AGENTS.md          | P0     | 10 min | DONE |
| T01   | 修复 `std::thread::sleep` 阻塞    | P0     | 5 min  | DONE |
| T02   | Tauri 命令批量异步化              | P0     | 30 min | DONE |
| T03   | 新建共享 `http_client` 模块       | P0     | 1 h    | DONE（验收已补完 by T11） |
| T04   | 后端事件推送 + 前端订阅           | P1     | 2 h    | DONE |
| T05   | `main.tsx` 机械拆分               | P1     | 2-4 h  | DONE |
| T06   | 引入 `ManagerError` 类型（试点）  | P1     | 半天   | DONE |
| T07   | 引入 `tracing` 框架               | P2     | 1 h    | DONE |
| T08   | Mutex 中毒处理                    | P2     | 1 h    | DONE |
| T09   | `storage.rs` 拆分调研（先不动手） | P2     | 1 h    | DONE |
| T10   | `provider.rs` 事务性调研          | P2     | 1 h    | DONE |
| T11   | 修复 `lib.rs` 测试 `default_upstream_protocol` import 遗漏 | P0 | 5 min | DONE |
| T12   | 补齐 `protocol_proxy_transport.rs` 漏掉的 3 处 `reqwest::Client::new()` | P1 | 10 min | DONE |
| T13   | 修复 `diagnostic_log` 并行测试 flaky | P2 | 30 min | DONE |
| T14   | 后端连接状态口径统一 + UI 文案修复 + ProgressDialog 改非阻塞 chip | P1 | 45 min | DONE |
| T14b  | 修复 Tauri 中 `window.confirm` 失效导致的前端阻塞 | P1 | 20 min | DONE |
| T14c  | 修复诊断页“后端状态”与启动页/总览页连接判定不一致 | P1 | 15 min | DONE |
| T15   | 修 .gitignore 把 docs/development 重要文档加白名单 | P2 | 5 min | DONE |

完成顺序建议：T00 → T01 → T02 → T03 → **T11** → T12 → T05 → T04 → T08 → T07 → T06 → T09 → T10 → T13。
T11 是 T03 验收阻塞项，必须先解。T12 是 T03 发现的漏网鱼，顺手收掉。
T13 是 T11 验收时暴露的 pre-existing flaky test，不阻塞主线，放最后。
T05 放在 T04 之前是因为前端拆完后 T04 的 listen 接入更干净。

---

## 已经做完的（不要再让 Codex 改）

避免 Codex 重复工作，先列出已建立的基础设施：

- `crates/codex-pilot-core/src/windows_integration.rs`：已有 `std_command` / `tokio_command` / `apply_no_window` / `spawn_hidden`
- `scripts/check-windows-hygiene.sh` + `scripts/lint-contracts.sh`：已禁裸 `Command::new` 和 runtime `println!`
- `.github/workflows/windows-verify.yml`：已有 Windows CI
- `lib.rs` 已从 2576 行拆到 416 行
- `protocol_proxy.rs` 已拆成 13 个文件
- 多数重操作已使用 `tauri::async_runtime::spawn_blocking`
- `docs/contracts/{subprocess,ipc,paths,logging,windows}.md` 契约文档已存在

---

## P0：会触发 UI 卡顿或死锁的实锤问题

### T00 · 把硬性约束加进 `AGENTS.md`

**问题**：现有 `docs/contracts/*.md` 内容已有，但 `AGENTS.md` 顶端没显式"必读"指令，Codex 写新代码时容易跳过契约。

**方案**：在 `AGENTS.md` 顶端追加硬性规则块。

**这一步你自己手动改，不需要 Codex**，10 分钟。在 `AGENTS.md` 文件开头插入：

```markdown
## 硬性约束（写任何代码前必读）

1. **必读**：`docs/contracts/subprocess.md`、`docs/contracts/ipc.md`、`docs/contracts/paths.md`、`docs/contracts/logging.md`、`docs/contracts/windows.md`

2. **Tauri 命令**：凡是涉及磁盘 I/O、网络 I/O、subprocess、调用 codex-pilot-data 的命令，必须 `async fn` + `tauri::async_runtime::spawn_blocking`，禁止同步 `fn` 内直接做这些操作。

3. **不要 `std::thread::sleep`**：`async fn` 里只能用 `tokio::time::sleep(...).await`。

4. **Mutex 不跨 `.await`**：`std::sync::Mutex` 不允许在持有期间出现 `await`。需要跨 await 用 `tokio::sync::Mutex` 或先 `.clone()` 释放。

5. **后端状态变化必须 emit Tauri 事件**：禁止依赖前端 polling 来感知状态机变化。前端 polling 只是兜底。

6. **新建 `reqwest::Client` 前**：先检查能不能用 `codex_pilot_core::http_client::shared()`。

7. **修一个 bug 不要顺手改其他东西**：每个 PR / commit 只对应 backlog 里一个任务编号。
```

完成后把本文件 T00 状态改为 DONE。

---

### T01 · 修复 `std::thread::sleep` 阻塞 tokio runtime

**问题**：`apps/codex-pilot-manager/src-tauri/src/commands/launch.rs:149` 在 `restart_codex_and_inject` 这个 `async fn` 里调 `std::thread::sleep(Duration::from_millis(1200))`，会卡住一个 tokio 工作线程 1.2 秒。

**Codex Prompt**：

```
修复 apps/codex-pilot-manager/src-tauri/src/commands/launch.rs:149 std::thread::sleep 阻塞 tokio runtime 的问题。

具体操作：在 restart_codex_and_inject 这个 async fn 里，把

    std::thread::sleep(std::time::Duration::from_millis(1200));

替换为

    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

只改这一行，不要顺手改其他东西。改完跑 cargo check -p codex-pilot-manager 确认通过。
```

**验收**：`cargo check -p codex-pilot-manager` 通过。

---

### T02 · 同步 Tauri 命令批量异步化

**问题**：以下 `#[tauri::command] fn`（同步）内部都在做磁盘 I/O，跑在 IPC 主循环上，会卡 UI：

- `apps/codex-pilot-manager/src-tauri/src/commands/provider.rs`：`provider_snapshot`、`ccs_provider_snapshot`、`import_official_snapshot_from_backup`、`prepare_official_snapshot_after_clearing_relay`、`import_ccs_provider_profiles`、`save_provider_profile`、`activate_provider_profile`、`delete_provider_profile`、`clear_provider`
- `apps/codex-pilot-manager/src-tauri/src/commands/app.rs`：`backend_status`、`save_launch_preferences`、`enhancement_settings_snapshot`、`save_enhancement_settings`

由于 `provider_snapshot` 和 `backend_status` 每次 refresh 都被调用（5 个 snapshot 并行），影响最大。

**Codex Prompt**：

```
把 apps/codex-pilot-manager/src-tauri/src/commands/provider.rs 和 apps/codex-pilot-manager/src-tauri/src/commands/app.rs 中以下 #[tauri::command] fn（同步）改为 #[tauri::command] async fn，内部用 tauri::async_runtime::spawn_blocking 包住原同步代码并 .await：

provider.rs 需要改的：
- provider_snapshot
- ccs_provider_snapshot
- import_official_snapshot_from_backup
- prepare_official_snapshot_after_clearing_relay
- import_ccs_provider_profiles
- save_provider_profile
- activate_provider_profile
- delete_provider_profile
- clear_provider

app.rs 需要改的：
- backend_status
- save_launch_preferences
- enhancement_settings_snapshot
- save_enhancement_settings

要求：
1. 不要修改函数的业务逻辑，只改签名 + spawn_blocking 包装。
2. 对原 Result<T, String> 返回的，在 .await 后把 JoinError 也转成 String 错误。
3. 前端 callBackend 调用不需要改（Tauri 自动适配）。
4. 如果原函数接收 tauri::State<'_, ManagerState>，注意 spawn_blocking 闭包不能直接捕获 State，需要先把内部需要的数据 clone 出来。
5. 改完跑 cargo check -p codex-pilot-manager 和 cd apps/codex-pilot-manager && npm test。
6. 不要修改其他文件，不要"顺手优化"无关代码。
```

**验收**：`cargo check` 通过、`npm test` 通过、启动 manager 后切换 view 不卡顿。

---

### T03 · 新建共享 `http_client` 模块

**问题**：`reqwest::Client` 在 5 处独立实例化（`cdp.rs:31`、`protocol_proxy_transport.rs:17/70/98`、`launcher.rs:162`），每次重建连接池 + TLS 握手。中转代理流量大时浪费明显。

**Codex Prompt**：

```
在 crates/codex-pilot-core/src/http_client.rs 新建模块，提供共享的 reqwest::Client。

要求：
1. 用 once_cell::sync::Lazy（如果 Cargo.toml 还没有 once_cell 依赖，加上）初始化一个全局 reqwest::Client。
2. 配置：connect_timeout(Duration::from_secs(5))、pool_idle_timeout(Duration::from_secs(60))、tcp_keepalive(Duration::from_secs(30))、user_agent 用 format!("CodexPilot/{}", crate::version::VERSION)。
3. 导出 pub fn shared() -> &'static reqwest::Client。
4. 在 crates/codex-pilot-core/src/lib.rs 加 pub mod http_client;。

然后替换以下 5 处的 client 创建为 http_client::shared()：
- crates/codex-pilot-core/src/cdp.rs:31
- crates/codex-pilot-core/src/protocol_proxy_transport.rs:17
- crates/codex-pilot-core/src/protocol_proxy_transport.rs:70
- crates/codex-pilot-core/src/protocol_proxy_transport.rs:98
- crates/codex-pilot-core/src/launcher.rs:162

注意事项：
- 只替换 client 创建那一行/一段，不动后续的 .request(...).header(...).send() 链式调用。
- launcher.rs:162 那处如果有特殊 timeout 配置（比如 launch timeout），**先不要合并到 shared client**，保留原 builder，在本任务的报告里单独说明，等我决定后再处理。
- 改完跑 cargo check 和 cargo test。
- 不要修改任何业务逻辑或请求参数。
```

**验收**：`cargo check` + `cargo test` 通过；启动后正常代理请求。

---

## P1：架构层让 Bug 反复出现的问题

### T04 · 后端 emit 事件 + 前端 listen 订阅

**问题**：`LaunchState` 在 `launch.rs` 里改了 8 次，但从不 emit Tauri event。前端只能通过 focus / visibilitychange 触发的 polling 感知状态机变化，导致中间态全靠"恰好这次 refresh 撞上"，是 reinject 假阳性 bug 的根源。

**Codex Prompt**：

```
在 apps/codex-pilot-manager/src-tauri/src/commands/launch.rs 给 LaunchState 状态变化加上 Tauri 事件推送。

后端改动：
1. 在每次给 state.launch_state 赋值（写入 LaunchState::Launching / Running / Failed / Idle）之后，立即调 app_handle.emit("launch_state_changed", launch_state_label(&new_state))。一共 8 处赋值，全部加上。
2. 如果当前函数签名拿不到 tauri::AppHandle，给函数加 app: tauri::AppHandle 参数；上游 #[tauri::command] 直接声明参数即可。
3. 不要改 LaunchState 枚举本身，不要改 launch_state_label 函数。

前端改动（apps/codex-pilot-manager/src/main.tsx）：
1. 引入 import { listen } from "@tauri-apps/api/event"。
2. 在 App 组件的初始化 useEffect 里订阅 listen("launch_state_changed", () => refresh(true))，记得在 cleanup 里 unlisten。
3. 不要删除现有 focus/visibilitychange refresh 监听（保留作为兜底）。

验收：
- cargo check + npm test 通过。
- 手动跑：启动 manager → 点启动 Codex → 不切窗口、不点刷新，UI 应在 1 秒内显示"运行中"，而不需要切到别的应用再切回来才更新。

不要修改其他无关代码。
```

**验收**：手动验证启动状态实时更新，不再需要切窗。

---

### T05 · `main.tsx` 机械拆分

**问题**：`apps/codex-pilot-manager/src/main.tsx` 2270 行、67 个 hook 调用全在一个 App 组件里。所有 useEffect 共享同一 closure，任何依赖数组漏一项就出 bug。竞态条件温床。

**Codex Prompt**：

```
重构 apps/codex-pilot-manager/src/main.tsx（当前 2270 行）。**用机械搬运法，禁止"优化"任何逻辑、禁止改 useEffect 依赖数组、禁止改 state 结构、禁止改 props 签名、禁止改任何 CSS class 名**。

当前 main.tsx 实际结构（不要相信任何与下面不一致的"猜测"，按下面来）：
- L32–240: 全部 type 定义（约 25 个 type/interface + 1 个 const THEME_STORAGE_KEY）
- L241: type ViewId
- L262–558: function App()
- L559–690: function OverviewView()
- L691–933: function LaunchView()
- L934–962: function SwitchRow()        ← 只被 LaunchView 用
- L963–1417: function ProviderView()
- L1418–2012: function RecycleBinView()  ← 这是 activeView==="sessions" 渲染的组件，名字就叫 RecycleBinView，**不要改名**
- L2013–2150: function DiagnosticsView()
- L2151–end: function Distribution() / Metric() / Row()  ← 被多个 view 用的小 primitive

callBackend 已经在 apps/codex-pilot-manager/src/backend.ts 里，**不要碰 backend.ts**。

按以下顺序执行，每步独立 commit + 跑验证：

【第 1 步】抽 types
- 新建 apps/codex-pilot-manager/src/types.ts
- 把 main.tsx 里 L32–L241 的全部 type/interface（含 ViewId）搬到 types.ts，**THEME_STORAGE_KEY 那个 const 也搬过去**
- main.tsx 顶部加 import { ...所有 type名... } from "./types";
- 跑 npm test && npm run build
- git add -A && git commit -m "T05.1: 抽出 types.ts"

【第 2 步】抽 primitives
- 新建 apps/codex-pilot-manager/src/components/primitives.tsx
- 把 main.tsx 末尾的 Distribution、Metric、Row 三个组件函数搬过去（连同它们的 props 类型，如果是 inline 的话）
- 在 primitives.tsx 顶部 import 需要的 types
- main.tsx 加 import { Distribution, Metric, Row } from "./components/primitives";
- 跑验证 + commit "T05.2: 抽出 primitives.tsx"

【第 3 步】每个 view 单独一步、单独 commit
按这个顺序（小的先做，风险低）：

  3a. OverviewView → apps/codex-pilot-manager/src/views/OverviewView.tsx
      （L559–690，含其 props type 如果是 inline 的）
      验证 + commit "T05.3a: 抽出 OverviewView"
  
  3b. DiagnosticsView → apps/codex-pilot-manager/src/views/DiagnosticsView.tsx
      验证 + commit "T05.3b: 抽出 DiagnosticsView"
  
  3c. LaunchView + SwitchRow → apps/codex-pilot-manager/src/views/LaunchView.tsx
      SwitchRow 只被 LaunchView 用，一起搬过去放同一文件
      验证 + commit "T05.3c: 抽出 LaunchView (含 SwitchRow)"
  
  3d. ProviderView → apps/codex-pilot-manager/src/views/ProviderView.tsx
      验证 + commit "T05.3d: 抽出 ProviderView"
  
  3e. RecycleBinView → apps/codex-pilot-manager/src/views/RecycleBinView.tsx
      文件名严格用 RecycleBinView.tsx（即使 ViewId 是 "sessions"，组件名跟着组件走）
      验证 + commit "T05.3e: 抽出 RecycleBinView"

【第 4 步】最终清理
- main.tsx 此时应只剩：imports + App 组件 + 文件末尾的 ReactDOM.createRoot 渲染调用
- 跑 npm test && npm run build 最后确认
- commit "T05.4: 收尾，main.tsx 仅保留 App"

每个 view 文件搬运规则（严格遵守）：
1. 组件函数原样复制过去，**props 列表不要解构合并，不要改名，不要 destructuring 优化**
2. 组件内部所有 useEffect / useState / useCallback / useMemo / useRef **原样照搬，依赖数组一个字都不要改**
3. 在新文件顶部 import 所需的：
   - React: `import * as React from "react";`
   - types: 从 ../types 引入
   - primitives: 从 ../components/primitives 引入
   - icons: lucide-react 原来引哪个引哪个
   - callBackend: 从 ../backend 引入
   - 其他 helper: 用到什么 import 什么
4. 如果 view 用到的 helper 函数（比如 formatXXX、parseXXX）当前在 main.tsx 顶部，把那个 helper 也一起搬到 view 文件里（不要为它们建独立 utils 文件，避免本任务范围扩大）。但如果是被多个 view 共用的 helper，先暂时复制一份到每个 view（**注意：要么全复制要么全引用，不要混用**）—— 在最终报告里列出这些被复制的 helper，留给后续清理任务。
5. main.tsx 删掉已搬走的代码段，加 `import { ViewName } from "./views/ViewName";`

绝对禁止的事：
- 不要改 App 组件
- 不要改 styles.css
- 不要改 backend.ts
- 不要改 autoLaunch.ts / autoLaunch.test.ts
- 不要改 dev/mockSnapshots.ts
- 不要改任何 JSX 内的 className 字符串
- 不要改任何 onClick / onChange 等事件 handler 的写法
- 不要"顺手"把 useEffect 拆开或合并
- 不要"顺手"修 ViewId 与组件名错配的问题（"sessions" → RecycleBinView 这个不一致**保留**，不要碰）

完成验收（最后一步必须做）：
- ls -la apps/codex-pilot-manager/src/ apps/codex-pilot-manager/src/views/ apps/codex-pilot-manager/src/components/
- wc -l apps/codex-pilot-manager/src/main.tsx apps/codex-pilot-manager/src/views/*.tsx apps/codex-pilot-manager/src/components/*.tsx apps/codex-pilot-manager/src/types.ts
  → 所有文件应 < 700 行（ProviderView 和 RecycleBinView 较大，700 行内可接受；其他应 < 400 行）
- npm test 全过
- npm run build 全过
- git log --oneline -10 给我看每一步独立 commit

完成后把 docs/development/refactor-backlog.md 状态总览表 T05 行 TODO 改 DONE。
```

**验收**：
- `main.tsx` 缩到约 300 行以内（只剩 App 组件 + 入口）
- `views/` 5 个文件齐全，单文件 ≤ 700 行
- 7 个独立 commit 可独立 revert
- `npm test` + `npm run build` 通过
- 手动跑一遍所有 view 点击行为不变（**这一项你自己跑**）

---

### T06 · 引入 `ManagerError` 类型（先在 `provider.rs` 试点）

**问题**：`Result<_, String>` 通行全栈（37 处）。前端无法分辨用户错误 / 系统错误 / 暂态错误，只能 toast 原始字符串。

**Codex Prompt**：

```
引入结构化错误类型 ManagerError，先在 provider.rs 一个文件试点。

后端改动：
1. 在 crates/codex-pilot-core/src/lib.rs 加 pub mod error;
2. 新建 crates/codex-pilot-core/src/error.rs：

   use serde::Serialize;
   use thiserror::Error;

   #[derive(Debug, Error, Serialize)]
   #[serde(tag = "kind", content = "detail")]
   pub enum ManagerError {
       #[error("{0}")] NotFound(String),
       #[error("{0}")] InvalidInput(String),
       #[error("{0}")] Conflict(String),
       #[error("{0}")] Io(String),
       #[error("{0}")] Internal(String),
   }

   如果 Cargo.toml 还没有 thiserror，请加上 workspace 依赖。

3. 把 apps/codex-pilot-manager/src-tauri/src/commands/provider.rs 中所有 Result<T, String> 改为 Result<T, ManagerError>，把现有 format!(...) 字符串包装为合适的 ManagerError 变体（读文件失败 → Io、配置不存在 → NotFound、名称重复 → Conflict、参数错误 → InvalidInput、其他 → Internal）。

4. 其他 .rs 文件先不改，只动 provider.rs。

前端改动（apps/codex-pilot-manager/src/main.tsx 或 utils）：
1. 修改 callBackend 的 catch 分支，如果错误对象是 { kind, detail } 结构，按 kind 分类：
   - InvalidInput → toast 高亮提示用户
   - NotFound / Conflict → 普通 toast
   - Io / Internal → toast + 建议去诊断页
2. 不需要改其他 view 的调用方。

验收：
- cargo check + npm test 通过。
- 手动测试：在 provider 页面触发一个失败（比如重名保存），UI 显示分类后的错误提示。
- 报告里说明哪些 String 错误归到了哪个 variant，方便后续讨论是否扩展到其他文件。
```

**验收**：provider 页面错误分类正确显示；其他页面无回归。

---

## P2：长期会咬人，不紧急

### T07 · 引入 `tracing` 框架

**问题**：现在没有标准 `log` / `tracing` crate，只有 `diagnostic_log::append`（追加 JSON 行，无 level、无 span、无 filter）。开发期无法"打开 debug 日志看一段"。

**Codex Prompt**：

```
引入 tracing 框架用于开发期日志，保留 diagnostic_log 不动（它是用户可见诊断，用途不同）。

步骤：
1. 在 workspace Cargo.toml 的 [workspace.dependencies] 加：
   tracing = "0.1"
   tracing-subscriber = { version = "0.3", features = ["env-filter"] }

2. 在 apps/codex-pilot-manager/src-tauri/Cargo.toml 加 tracing.workspace = true; tracing-subscriber.workspace = true;
   在 crates/codex-pilot-core/Cargo.toml 加 tracing.workspace = true;

3. 在 apps/codex-pilot-manager/src-tauri/src/lib.rs 的 .setup(|app| { ... }) 或 main 入口最早处加：
   tracing_subscriber::fmt()
       .with_env_filter(std::env::var("CODEX_PILOT_LOG").unwrap_or_else(|_| "info,codex_pilot=debug".into()))
       .init();

4. 试点：在 crates/codex-pilot-core/src/bridge.rs 里每个 let _ = diagnostic_log::append(...) 旁边补一行 tracing::debug!(target = "bridge", "<事件名>: ..."); 镜像关键事件即可，不要镜像全部。

5. 不修改 diagnostic_log 本身。

验收：cargo check 通过；启动时设 CODEX_PILOT_LOG=debug 能看到 bridge debug 日志。
```

**验收**：能通过环境变量控制日志级别。

---

### T08 · Mutex 中毒处理

**问题**：`launch.rs` 大量 `if let Ok(mut current) = state.launch_state.lock() { *current = LaunchState::Failed(...) }` ——锁中毒时**静默丢失状态更新**，前端永远不知道。

**Codex Prompt**：

```
修复 apps/codex-pilot-manager/src-tauri/src/commands/launch.rs 和 launch_helpers.rs 中静默丢失 Mutex 中毒错误的问题。

具体改动：
1. 把所有 `if let Ok(mut <var>) = state.launch_state.lock() { ... }` 这种"中毒就静默"的写法，统一改成下面的辅助函数模式：

   在 launch_helpers.rs 加：

   pub(crate) fn with_launch_state_mut<F>(state: &ManagerState, f: F)
   where F: FnOnce(&mut LaunchState),
   {
       match state.launch_state.lock() {
           Ok(mut guard) => f(&mut guard),
           Err(poisoned) => {
               tracing::error!("launch_state mutex poisoned, recovering");
               let mut guard = poisoned.into_inner();
               f(&mut guard);
               state.launch_state.clear_poison();
           }
       }
   }

2. 把 launch.rs 里所有"静默丢弃"的写法（具体是 line 180、263、274、291 等用 if let Ok 的地方）替换成调用 with_launch_state_mut。
3. 持有锁正常路径（map_err(|_| "启动状态锁已损坏") 的 ?）保持不动——那些会把错误返回给前端，不算静默丢失。
4. codex_process_cache 也加一个对应的 with_codex_process_cache_mut，同样处理。

注意：tracing crate 需要先做 T07，如果 T07 还没完成，先用 eprintln! 临时占位（但 lint-contracts.sh 会报 println!，所以最好等 T07 完成后再做 T08）。

验收：cargo check + cargo test 通过。
```

**验收**：cargo 通过；故意触发 panic 后状态机仍能恢复。

---

### T09 · `storage.rs` 拆分前的调研

**问题**：`crates/codex-pilot-data/src/storage.rs` 1957 行。直接让 Codex 拆容易出错。先做调研，**不动代码**。

**Codex Prompt**：

```
对 crates/codex-pilot-data/src/storage.rs（1957 行）做拆分调研。本任务 ABSOLUTELY 不写任何拆分代码，只做分析。

请输出一份报告，包含：

1. 文件里所有 pub fn 和 pub struct 的列表，每项标注：
   - 名称
   - 行号范围
   - 简短功能描述（一句话）
   - 主要依赖（其他 pub item 或外部 crate）

2. 按照功能分组建议（每组 3-8 项）：
   - 会话读写
   - 备份归档
   - 索引维护
   - 路径解析
   - 其他（如果有）

3. 各组之间的依赖关系（A 组依赖 B 组哪些函数）。

4. 识别可能的拆分风险：
   - 私有 helper 被多组共用，拆分后变成 pub
   - 共享 const / state
   - 测试模块需不需要跟拆

输出格式：markdown 报告，保存到 docs/development/storage-split-analysis.md。

不要修改 storage.rs 任何代码。不要新建任何 .rs 文件。
```

**验收**：拿到分析文档，由你（人）决定拆分计划，再开 T09b 任务。

---

### T10 · `provider.rs` 副作用 / 事务性调研

**问题**：`save_provider_profile`、`apply_provider` 等函数中途失败会留下"profiles 已写但中转未应用"的不一致状态。

**Codex Prompt**：

```
对 apps/codex-pilot-manager/src-tauri/src/commands/provider.rs 中 save_provider_profile 和 apply_provider 这两个函数做副作用调研。本任务 ABSOLUTELY 不写任何修改代码。

请输出报告：

1. 对每个函数，列出从开始到 return 的每一步副作用：
   - 写哪个文件？
   - 改哪个系统状态（环境变量 / config / 备份目录）？
   - 调用了哪些其他模块（codex_pilot_core::relay_config 等）？
   - 这一步是否可回滚？回滚方式是什么？

2. 标注每一步的失败模式：
   - 这一步失败会留下什么不一致？
   - 重试是否安全（幂等性）？

3. 给出 3 个可选事务性方案的对比（先实现哪个最小代价、影响面、对现有代码改动量）：
   - 方案 A：写之前 snapshot 一份，失败时 rollback
   - 方案 B：拆成"准备 + 提交"两阶段，前端先调 prepare 再调 commit
   - 方案 C：什么都不做，仅在文档里写明"中途失败需手动恢复 X 文件"

输出保存到 docs/development/provider-transaction-analysis.md。
```

**验收**：拿到调研文档，由你决定走 A/B/C 哪个方案。

---

## 任务模板（以后新增任务用）

写新任务时套这个模板：

```markdown
### T?? · 标题

**问题**：（事实 + 具体代码位置 file:line）

**Codex Prompt**：

\`\`\`
（自包含的完整指令，不要让 Codex"先看全局"）
要求列表：
1. ...
2. ...
不要做的事：
- ...
验收：
- cargo check / npm test
- 手动验证步骤
\`\`\`

**验收**：（一句话，描述完成标志）
```

**写 Prompt 的硬性规则**：
1. 给出具体文件路径 + 行号
2. 列出"要做"和"不要做"
3. 明确验收命令
4. 禁止"顺手优化"无关代码
5. 一个任务一个 PR / commit
6. Codex 不要看全局——一切上下文都在 prompt 里

---

### T11 · 修复 `lib.rs` 测试 `default_upstream_protocol` import 遗漏

**问题**：`apps/codex-pilot-manager/src-tauri/src/lib.rs:343 和 352` 在 `#[cfg(test)] mod tests` 内引用 `default_upstream_protocol()`，该函数定义在 `apps/codex-pilot-manager/src-tauri/src/provider_store_types.rs:100`，且为 `pub(crate)`。测试 mod 没显式 `use`，导致 `cargo test --workspace` 编译失败。

错误信息：
```
error[E0425]: cannot find function `default_upstream_protocol` in this scope
   --> apps/codex-pilot-manager/src-tauri/src/lib.rs:343:40
help: consider importing this function
    |
227 +     use crate::provider_store_types::default_upstream_protocol;
```

这是 lib.rs 拆分时遗漏的 import，与 T03 无关，但阻塞 T03 验收。

**Codex Prompt**：

```
修复 apps/codex-pilot-manager/src-tauri/src/lib.rs 测试模块 cargo test --workspace 编译失败。

具体操作：在 lib.rs 的 #[cfg(test)] mod tests { ... } 块开头（紧跟 use super::*; 之后），新增一行：

    use crate::provider_store_types::default_upstream_protocol;

只加这一行。不要修改 use super::*; 那行，不要改 default_upstream_protocol 函数本身，不要改其他任何文件。

改完跑：
- cargo check --workspace --tests
- cargo test --workspace

两者都必须通过。完成后把 docs/development/refactor-backlog.md 状态总览表 T11 行的 TODO 改为 DONE，同时在 T03 行的状态后追加"（验收已补完 by T11）"。
```

**验收**：`cargo test --workspace` 全绿。

---

### T12 · 补齐 `protocol_proxy_transport.rs` 漏掉的 3 处 `reqwest::Client::new()`

**问题**：T03 只替换了原 prompt 列出的 5 处，但 `crates/codex-pilot-core/src/protocol_proxy_transport.rs` 实际还有 3 处独立 `reqwest::Client::new()` 在用：
- line 31：`reqwest::Client::new().get(models_url(&target.base_url))`
- line 42：同样
- line 53：同样

Codex 按 T03 prompt 严格执行没碰这 3 处（正确行为），但它们也应该用 shared client。

**Codex Prompt**：

```
T03 已经在 crates/codex-pilot-core/src/http_client.rs 建好共享 reqwest::Client，导出 http_client::shared() -> &'static reqwest::Client。

现在补齐 crates/codex-pilot-core/src/protocol_proxy_transport.rs 中遗漏的 3 处 reqwest::Client::new() 替换：

- line 31: `let mut request = reqwest::Client::new().get(...);`
- line 42: `let mut request = reqwest::Client::new().get(...);`
- line 53: `let mut request = reqwest::Client::new().get(...);`

每处把 `reqwest::Client::new()` 替换为 `crate::http_client::shared()`，注意 shared() 返回 &Client 引用，调用方式：`crate::http_client::shared().get(...)` 即可。

要求：
- 只改这 3 处，不要碰这 3 处之外的任何代码
- 不要修改 http_client.rs
- 改完跑 cargo check 和 cargo test --workspace（需要 T11 已 DONE 才能跑 test）

完成后把 backlog 文档 T12 行的 TODO 改为 DONE。
```

**验收**：`cargo check` + `cargo test --workspace` 通过；全项目 `grep -n "reqwest::Client::" crates/codex-pilot-core/src/` 只在 `http_client.rs` 和 `launcher.rs:162`（特殊例外）出现。

---

### T13 · 修复 `diagnostic_log` 并行测试 flaky

**问题**：`crates/codex-pilot-core/src/diagnostic_log.rs::tests::append_rotates_large_log_file` 在 `cargo test --workspace`（默认并行）下偶尔失败，断言 `rotated_path(&path, 1).exists()` 不通过；改 `--test-threads=1` 则稳定通过。

根因分析：
- 模块用一个 `static TEST_LOG_PATH: Mutex<Option<PathBuf>>` 让测试可覆盖 `log_path()`
- 3 个测试用 `test_log_guard()` 内部锁串行化，理论上应该够
- 但测试写 20MB+ 文件触发 rotate，并行运行时多个测试同时进行大文件 IO + 全局静态切换，导致竞态
- 这是 T11 完成后 cargo test 首次能跑起来时暴露的 pre-existing 隐患，与 T11 无关

**Codex Prompt**：

```
修复 crates/codex-pilot-core/src/diagnostic_log.rs 测试在并行执行下 flaky 的问题。

先调研（不写代码）输出结论给我：
1. 用 cargo test -p codex-pilot-core --lib diagnostic_log 跑 10 次，记录 pass/fail 比例
2. 用 cargo test -p codex-pilot-core --lib diagnostic_log -- --test-threads=1 跑 10 次，记录比例
3. 分析失败模式：是 rename 没生效、还是 metadata 读到旧值、还是其他时序问题
4. 给出 2-3 个候选修复方案，比较优劣：
   方案 A: 把 MAX_LOG_BYTES 改成可被测试 override 的参数，测试用 8 KB 而不是 20 MB（减小 IO 时间窗口）
   方案 B: 在 test_log_guard() 改成 std::sync::Mutex<()> 之外，再加 fs::sync_data 等强制刷盘
   方案 C: 把这 3 个 test 标记 #[serial_test::serial]（引入 serial_test crate）
   方案 D: 其他

只输出报告到 docs/development/diagnostic-log-flaky-analysis.md，不写任何修改代码。我看完报告后再决定走哪个方案。
```

**验收**：拿到调研报告；由维护者选定方案后再起 T13b。

---

### T14 · 后端连接状态口径统一 + UI 文案修复 + ProgressDialog 改非阻塞 chip

**问题**：UI 一组相关显示 bug，本质都是"用户能否在长任务进行中继续操作"+ "状态显示口径不统一"。一起修：

1. **诊断页"后端状态"**（`apps/codex-pilot-manager/src-tauri/src/commands/diagnostics.rs:17-32`）：
   - 当前逻辑：`status="ok"` 条件是 `helper_reachable || status_exists`
   - 当 `helper_reachable=false && status_exists=true` 时：status 仍是 "ok"，但 detail 文案明确说"未检测到本地连接服务"——自相矛盾
   - 真实语义：状态文件可能是上次启动残留，**helper 端口不通就不应该算 OK**

2. **启动页"连接方式"**（`apps/codex-pilot-manager/src/views/LaunchView.tsx:37`）：
   - 当前：`const connectionState = launch?.debugReachable ? "可直接注入" : launch?.codexRunning ? "需要重启注入" : "可启动";`
   - 完全只看 debugReachable，没区分"完全已连接"和"helper 未通但 debug 通"
   - 而后端已经算好 `actionKind`（running/launching/reinject/restart/launch/unavailable），前端不该再造一遍轮子

3. **右上角主按钮**（`apps/codex-pilot-manager/src/main.tsx:286-288`）：
   - 当 actionKind="running" 时，按钮文字"已运行" + Play 图标 + enabled（因为 "running" 在 canRunLaunchAction 白名单里）
   - 视觉上像可点的启动按钮，点了走快速返回路径无害但毫无意义；语义应为状态指示而非可点按钮

4. **ProgressDialog 全屏遮罩拦截所有点击**（`apps/codex-pilot-manager/src/styles.css:467-475` + `apps/codex-pilot-manager/src/appSupport.tsx:21-33`）：
   - 当前 `.progressOverlay` 是 `position:fixed; inset:0; z-index:1100`，**显示期间盖住整个 UI**
   - 后端 `export_session_zip` / `sync_provider_sessions` 等已是 spawn_blocking async，后端不阻塞；**是前端自己用 modal 把自己锁死了**
   - 后果：用户在导出过程中点"永久删除"按钮根本接收不到 click（被遮罩拦截）→ 误以为按钮坏了
   - 真实情况：当前所有用 onProgress 的操作都是并发安全的后端任务，不需要 modal 遮罩

**Codex Prompt**：

```
修复 CodexPilot 一组 UI 状态显示与交互阻塞问题。本任务范围只在这 5 个文件：

文件 1: apps/codex-pilot-manager/src-tauri/src/commands/diagnostics.rs
文件 2: apps/codex-pilot-manager/src/views/LaunchView.tsx
文件 3: apps/codex-pilot-manager/src/main.tsx
文件 4: apps/codex-pilot-manager/src/appSupport.tsx
文件 5: apps/codex-pilot-manager/src/styles.css

【改动 1】diagnostics.rs 后端状态语义修正
当前 line 17-32 的逻辑：status="ok" 当 helper_reachable || status_exists。
改为三档：
- helper_reachable=true → status="ok", detail="本地连接服务已连接；状态文件路径：{path}"
- helper_reachable=false && status_exists=true → status="warning", detail="本地连接服务无响应，但发现旧状态文件：{path}。后端可能已退出或端口配置不一致，请回到启动页点'重新注入'。"
- helper_reachable=false && !status_exists → status="missing", detail="未检测到本地连接服务，且状态文件不存在：{path}"

注意 DiagnosticCheck.status 当前只用 ok/warning/missing 三个字符串。如果 DiagnosticCheck type 或前端 status 渲染不支持 "warning"，先用现有支持的 status 类型（看现有其他 check 如"中转设置"和"Codex 应用探测"用的字符串）。如不支持 warning，用 "missing"+清楚的 detail 也可以。

【改动 2】LaunchView.tsx:37 用后端 actionKind 派生 connectionState
把
    const connectionState = launch?.debugReachable ? "可直接注入" : launch?.codexRunning ? "需要重启注入" : "可启动";
替换为基于 launch?.actionKind 的 switch（或三元链）：
    const connectionState = (() => {
      switch (launch?.actionKind) {
        case "running":      return "已连接";
        case "launching":    return "启动中";
        case "reinject":     return "可直接注入";
        case "restart":      return "需要重启注入";
        case "launch":       return "可启动";
        case "unavailable":  return "未配置";
        default:             return "未知";
      }
    })();

不要改 LaunchView 其他任何代码。

【改动 3 + 4】右上角按钮在 actionKind="running" 时改为禁用状态指示
appSupport.tsx 修改 canRunLaunchAction：把 "running" 从白名单移除：
    return ["launch", "reinject", "restart"].includes(launch.actionKind);

然后 main.tsx 的按钮 JSX（约 line 286-289）做小幅调整：
- 图标：actionKind === "running" 时用 lucide-react 的 CheckCircle2，其他保持原逻辑
- 文字：launching → "处理中"；actionKind === "running" → "已连接"；其他用 launch?.actionLabel
- 不需要改 disabled 逻辑（canRunLaunchAction 移除 running 后会自然 disabled）

记得在 main.tsx import 处新增 CheckCircle2。

【改动 5】ProgressDialog 改成右下角非阻塞 chip

styles.css：
- 删除整段 `.progressOverlay { ... }`（约 line 467-475）
- 新增 `.progressChip` 样式：
    .progressChip {
      animation: toastIn 140ms ease-out;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 8px;
      box-shadow: 0 14px 32px var(--shadow-strong);
      bottom: 70px;        /* 让出底部 22px 给 .appToast */
      display: grid;
      gap: 8px;
      max-width: min(360px, calc(100vw - 36px));
      padding: 12px 14px;
      pointer-events: none;
      position: fixed;
      right: 22px;
      z-index: 1000;
    }
- 把 `.progressDialog` 和 `.progressDialog strong` / `.progressDialog p` / `.progressTrack` 等子选择器全部改成 `.progressChip *` 对应版本（或者直接复用现有 progressTrack 动画）。
  关键：取消"模态阴影"、取消居中布局、取消全屏覆盖。

appSupport.tsx 修改 ProgressDialog 组件（约 line 21-33）：
- JSX 外层 div 的 className 从 "progressOverlay" 改为 "progressChip"
- 删除内层 .progressDialog 包装（直接渲染 strong + progressTrack 即可）
- 不需要 aria-live 还是保留作 a11y
- 组件名可保留 ProgressDialog（导出名不变，避免 main.tsx import 改动），但实际渲染的是 chip 样式

示例改后：
    export function ProgressDialog({ message }: { message: string }) {
      return (
        <div className="progressChip" role="status" aria-live="polite">
          <strong>{message}</strong>
          <div className="progressTrack"><span /></div>
        </div>
      );
    }

不要做的事：
- 不要改 launch_action_kind / launch_action_label 后端逻辑（actionKind 已经对了）
- 不要改 backendStatusLabel
- 不要改其他 view 文件
- 不要改 autoLaunch.ts 或 autoLaunch.test.ts
- 不要把 ProgressDialog 改名（保持导出名兼容）
- 不要改任何 onProgress() 调用点（chip 应替换为 modal 透明无痛升级）

完成后给我：
1. git diff --stat（应只看到 5 个源文件 + backlog 文档）
2. 改动后的 LaunchView.tsx:37 那段 connectionState 完整代码
3. 改动后的 diagnostics.rs 后端状态 check 完整段落
4. 改动后的 main.tsx 按钮 JSX
5. 改动后的 appSupport.tsx canRunLaunchAction + ProgressDialog 完整代码
6. 改动后的 styles.css 中 .progressChip 完整定义
7. cargo check -p codex-pilot-manager 输出
8. cd apps/codex-pilot-manager && npm test 输出
9. npm run build 输出
10. 把 backlog T14 行 TODO 改 DONE
```

**验收**：
- 当 helper 端口通时：启动页"后端：已连接"、"连接方式：已连接"，诊断页"后端状态：OK"，右上按钮"已连接"+CheckCircle2 图标+disabled
- 当 helper 端口不通但 status 文件存在：诊断页"后端状态：warning（或 missing）"，detail 清楚说明"后端可能已退出"
- 当真正未启动：右上按钮"启动 Codex"+Play+enabled
- 触发导出 ZIP / 同步对话等长任务：右下角出现一个 chip 显示进度，**同时其他按钮（永久删除、刷新等）仍可点击**
- chip 不遮挡 toast（toast 在底部 22px、chip 在底部 70px 不重叠）
