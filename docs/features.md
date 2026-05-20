# CodexPilot 功能说明

这里解释 CodexPilot 每个页面能做什么、会读写哪些本地数据，以及哪些操作需要先预览影响范围。README 只保留首页和快速入口；完整功能说明放在这里维护。

## 目录

- [启动与注入](#启动与注入)
- [会话导出与维护](#会话导出与维护)
- [Timeline](#timeline)
- [模型通道](#模型通道)
- [Provider 归属同步](#provider-归属同步)
- [诊断](#诊断)
- [本地数据与安全](#本地数据与安全)
- [兼容性说明](#兼容性说明)

## 启动与注入

CodexPilot 使用本地 launcher 启动 Codex，并通过 Chromium DevTools Protocol 连接页面。注入成功后，Codex 页面会出现 CodexPilot 操作菜单。

如果 Codex 已经由其他方式启动，管理器会根据当前状态提示重新注入或重启。重启 Codex 前会要求确认，避免未保存输入意外丢失。

![CodexPilot 启动页面](images/readme-launch.png)

## 会话导出与维护

CodexPilot 可以在普通会话和归档会话中提供额外操作：

- 导出 Markdown。
- 删除会话。
- 短时撤销删除。
- 查看、恢复或永久清理回收站中的删除备份。
- 批量删除归档会话。

删除和恢复操作会读写本机 Codex 的会话数据库。CodexPilot 会尽量保留可恢复备份，但仍建议在批量清理前确认会话内容已经不再需要。

![CodexPilot 对话维护页面](images/readme-recycle-bin.png)

## Timeline

在当前 Codex 对话中，如果检测到至少两个用户提问，CodexPilot 会在页面右侧显示一条轻量 Timeline。每个圆点代表一个用户提问，悬停可以查看问题摘要，点击会把对应提问滚动到屏幕中间。

Timeline 只读取当前页面内容，不写入会话文件、状态数据库或配置文件。如果当前页面不是对话页、无法识别会话，或用户提问数量不足，Timeline 会自动隐藏。

## 模型通道

### 混合中转

混合中转适合已经在 Codex/ChatGPT 中完成官方登录，同时希望模型请求走自定义兼容 API 的场景。它的重点不是简单“换一个 API”，而是在保留官方登录链路的同时使用自己的中转站：你仍然可以用手机 ChatGPT 控制或接续桌面 Codex，桌面 Codex 的模型请求则走你配置的自定义 Provider。

![CodexPilot 模型通道页面](images/readme-provider.png)

Base URL 和 API Key 来自第三方或自建的 OpenAI-compatible API。官方登录态用于保持 Codex/ChatGPT 登录兼容和跨端控制体验；启用混合中转后，模型请求会发送给你配置的自定义 Provider，隐私、计费和数据处理策略以该 Provider 为准。

使用步骤：

1. 先用原版 Codex 完成 ChatGPT 登录。
2. 打开 CodexPilot 管理器，进入“模型通道”。
3. 新增或选择一个中转配置档。
4. 填写 Base URL 和 API Key，保存配置。
5. 选择“混合中转”并点击“保存”。
6. 从 CodexPilot 启动或重新注入 Codex。

通常不需要手动编辑配置文件。CodexPilot 会写入 `~/.codex/config.toml`，配置形态类似：

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

### 官方通道

在管理器“模型通道”页面选择“官方通道”并点击“保存”，CodexPilot 会：

- 删除 `CodexPilot` provider 配置。
- 移除根级 `OPENAI_API_KEY`。
- 将 `model_provider` 切回 `chatgpt`。
- 写入前保留配置备份。

如果你原本手动在 `~/.codex/config.toml` 根级写过 `OPENAI_API_KEY`，切回官方通道也会移除它。写入前会保留配置备份。

## Provider 归属同步

切换 provider 后，历史会话可能因为 `model_provider` 不一致而不可见或分组异常。CodexPilot 不再自动改写历史会话归属；如需整理历史数据，可在管理器“对话维护”页使用“对话归属同步”，先预览影响范围，再手动同步到选定 Provider。

如果只是临时切换模型通道，或者不确定预览结果里的影响范围，先不要同步。这个功能适合在历史会话不可见、分组异常，且你确认希望把这些历史记录归到目标 Provider 时使用。

同步范围：

- `~/.codex/sessions/**/rollout-*.jsonl`
- `~/.codex/archived_sessions/**/rollout-*.jsonl`
- `~/.codex/state_5.sqlite`
- `~/.codex/.codex-global-state.json`

备份位置：

```text
~/.codex/backups_state/provider-sync/
```

## 诊断

管理器会展示启动、注入、中转和页面连接相关检查项，也可以导出诊断日志，方便定位问题或提交反馈。

![CodexPilot 诊断页面](images/readme-diagnostics.png)

诊断信息主要用于判断：

- Codex 应用路径是否可用。
- 调试端口和后端端口是否正常。
- 页面是否已经连接并完成注入。
- 当前模型通道配置是否完整。
- 会话维护和 Provider 同步所需的本地数据是否可访问。

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

CodexPilot 还会使用本机 loopback 调试端口和本地 helper 端口。Chromium DevTools Protocol 连接具备页面脚本执行能力，请只在可信本机环境使用。

补充数据位置：

- `~/.codex/config.toml.codex-pilot-backup-*.bak`：写入模型通道配置前保留的配置备份，可能包含旧 API Key。
- `~/.codex/.codex-pilot-undo/`：删除会话后的撤销/回收站备份。
- CodexPilot 应用状态目录中的 `provider-profiles.json`：中转配置档，包含 Base URL 和 API Key。macOS/Linux 通常在 `~/.config/CodexPilot/`，Windows 通常在 `%APPDATA%\CodexPilot\`。

## 兼容性说明

CodexPilot 依赖 Codex App 的页面结构和本地数据格式。Codex App 更新后，如果页面结构、会话数据库或配置格式发生变化，可能需要更新 CodexPilot 的页面连接脚本或同步逻辑。
