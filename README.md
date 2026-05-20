# CodexPilot

CodexPilot 是 Codex App 的外部增强控制台。它通过本地启动器打开 Codex，并使用 Chromium DevTools Protocol 连接运行中的页面，为 Codex 补上会话导出、会话维护、混合中转和诊断能力。

CodexPilot 不修改 Codex App 安装目录。

> CodexPilot 是非官方工具，不隶属于 OpenAI 或 Codex App。

![CodexPilot manager UI preview](assets/screenshots/manager-ui-preview.png)

## 它能做什么

- 从桌面管理器启动 Codex，并注入 CodexPilot 操作菜单。
- 在 Codex 页面中导出当前会话为 Markdown。
- 删除会话、短时撤销删除，并在管理器里查看和清理回收站。
- 支持归档会话的导出、删除和批量删除。
- 在保留官方 ChatGPT 登录态的同时，把模型请求切到自定义兼容 API。
- 管理多个中转配置档，并在切换 provider 后同步历史会话元数据。
- 收集启动、注入、页面连接、路由和中转配置相关的诊断日志。

## 适合谁

CodexPilot 适合已经在使用 Codex App，并且希望获得这些能力的用户：

- 把重要会话导出成可归档、可检索的 Markdown。
- 更方便地维护普通会话和归档会话。
- 使用自定义兼容 API，同时继续保留 Codex/ChatGPT 官方登录态。
- 排查 Codex 启动、页面注入或中转配置问题。

如果你只需要原版 Codex App 的标准体验，不需要会话维护或中转能力，可以继续直接使用原版应用。

## 安装

当前 GitHub Release 自动发布 Windows 安装包。macOS 包由维护者在本地发布环境中构建，Release 中是否提供以具体版本资产为准。

### 直接安装

从 [GitHub Releases](https://github.com/hl9565/CodexPilot/releases) 下载对应平台安装包：

- Windows：`CodexPilot-*-windows-x64-setup.exe`
- macOS Intel：`CodexPilot-*-macos-x64.dmg`（如该版本提供）
- macOS Apple Silicon：`CodexPilot-*-macos-arm64.dmg`（如该版本提供）

macOS 如提供 DMG，打开后把 `CodexPilot.app` 拖入 Applications。Windows 运行安装程序后会创建桌面和开始菜单快捷方式。

安装完成后打开 CodexPilot 管理器，再从管理器启动 Codex。

### 源码运行

从源码运行需要先安装 Rust、Node.js 和 npm：

```bash
cd apps/codex-pilot-manager
npm install
npm run dev
```

源码运行适合本地调试和临时使用，不需要先打包成 DMG。

## 交流与支持

<img width="313" height="481" alt="微信图片_20260520102952_77_276" src="https://github.com/user-attachments/assets/ca69b9b2-64f9-461d-b81b-7f1a3b0eb6b9" />

## 快速开始

1. 先安装并正常登录原版 Codex App。
2. 打开 CodexPilot 管理器。
3. 在“启动与注入”里确认 Codex 应用路径和端口状态。
4. 点击“启动 Codex”。
5. 在 Codex 页面右下角使用 CodexPilot 菜单，或回到管理器使用模型通道、会话维护和诊断功能。

## 功能说明

### 启动与注入

CodexPilot 使用本地 launcher 启动 Codex，并通过 Chromium DevTools Protocol 连接页面。注入成功后，Codex 页面会出现 CodexPilot 操作菜单。

如果 Codex 已经由其他方式启动，管理器会根据当前状态提示重新注入或重启。重启 Codex 前会要求确认，避免未保存输入意外丢失。

### 会话导出与维护

CodexPilot 可以在会话行和归档会话页面添加额外操作：

- 导出 Markdown。
- 删除会话。
- 短时撤销删除。
- 查看、恢复或永久清理回收站中的删除备份。
- 批量删除归档会话。

删除和恢复操作会读写本机 Codex 的会话数据库。CodexPilot 会尽量保留可恢复备份，但仍建议在批量清理前确认会话内容已经不再需要。

### 混合中转

混合中转适合已经在 Codex/ChatGPT 中完成官方登录，同时希望模型请求走自定义兼容 API 的场景。

使用步骤：

1. 先用原版 Codex 完成 ChatGPT 登录。
2. 打开 CodexPilot 管理器，进入“模型通道”。
3. 新增或选择一个中转配置档。
4. 填写 Base URL 和 API Key，保存配置。
5. 选择“混合中转”并点击“保存”。
6. 从 CodexPilot 启动或重新注入 Codex。

CodexPilot 会写入 `~/.codex/config.toml`，配置形态类似：

```toml
model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://example.com/v1"
experimental_bearer_token = "sk-..."
```

如果没有检测到 `~/.codex/auth.json` 中的 ChatGPT 登录态，CodexPilot 会拒绝保存混合中转配置。

### Provider Sync

切换 provider 后，历史会话可能因为 `model_provider` 不一致而不可见或分组异常。CodexPilot 在保存混合中转后，以及启动 Codex 前，会自动同步本地会话元数据。

同步范围：

- `~/.codex/sessions/**/rollout-*.jsonl`
- `~/.codex/archived_sessions/**/rollout-*.jsonl`
- `~/.codex/state_5.sqlite`
- `~/.codex/.codex-global-state.json`

备份位置：

```text
~/.codex/backups_state/provider-sync/
```

### 官方通道

在管理器“模型通道”页面选择“官方通道”并点击“保存”，CodexPilot 会：

- 删除 `CodexPilot` provider 配置。
- 移除根级 `OPENAI_API_KEY`。
- 将 `model_provider` 切回 `chatgpt`。
- 写入前保留配置备份。

### 诊断

管理器会展示启动、注入、中转和页面连接相关检查项，也可以导出诊断日志，方便定位问题或提交反馈。

## 本地数据与安全

CodexPilot 会读取或写入以下本机位置：

- `~/.codex/config.toml`：中转配置。
- `~/.codex/auth.json`：只用于检测官方登录态。
- `~/.codex/sessions/`：会话元数据和导出来源。
- `~/.codex/archived_sessions/`：归档会话元数据和导出来源。
- `~/.codex/state_5.sqlite`：会话索引、删除、恢复和 provider 同步。
- `~/.codex/backups_state/provider-sync/`：Provider Sync 备份。
- CodexPilot 自己的应用状态目录：启动偏好、中转配置档、诊断日志。

中转配置档会保存在本机。API Key 不会显示在状态面板里，但会保存到本地配置文件。请只在可信设备上使用，并避免把本地配置、日志、截图或备份目录上传到公开仓库。

使用自定义兼容 API 时，请自行确认服务提供方的隐私、计费和数据处理策略。

## 开发

```bash
cargo test
node scripts/test-renderer-inject.mjs

cd apps/codex-pilot-manager
npm install
npm run check
```

### 管理器 UI 预览

改管理器界面时，可以直接在浏览器里打开开发期预览，不需要启动完整 Tauri 桌面壳：

```bash
cd apps/codex-pilot-manager
npm run preview:ui
```

然后打开 `http://127.0.0.1:1420`。预览模式会使用本地 mock 数据，覆盖启动、模型通道、回收站和诊断页面；外层窗口默认使用真实 App 配置里的 `1120x760` 尺寸，方便检查 UI 在实际桌面窗口中的表现。

## 发布前检查

- `cargo fmt`
- `cargo test`
- `cargo check`
- `node scripts/test-renderer-inject.mjs`
- `npm run check`
- 真实 Codex 登录态下验证混合中转保存、启动、历史会话可见性和新会话请求。
- 检查日志、截图、测试数据中没有真实密钥。

## 打包与发布

公开仓库保留 Windows 自动发布流程。发布 GitHub Release 后，Actions 会在 Windows runner 上构建并上传 `CodexPilot-*-windows-x64-setup.exe`。

macOS 安装包由维护者在本地发布环境中构建，并按需上传到 GitHub Releases。

如果你想自行打包，可以参考 Tauri、Rust 和 Node.js 的官方流程构建 `codex-pilot-manager`，并确保 `codex-pilot-launcher` 作为 sidecar 放入 `apps/codex-pilot-manager/src-tauri/binaries/`。

## 兼容性说明

CodexPilot 依赖 Codex App 的页面结构和本地数据格式。Codex App 更新后，如果页面结构、会话数据库或配置格式发生变化，可能需要更新 CodexPilot 的页面连接脚本或同步逻辑。

## License

MIT
