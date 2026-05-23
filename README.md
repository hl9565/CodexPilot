<p align="center">
  <img src="apps/codex-pilot-manager/src-tauri/icons/icon.png" width="96" height="96" alt="CodexPilot icon" />
</p>

<h1 align="center">CodexPilot</h1>

<p align="center">
  让 Codex 的本地工作流更顺手、更可控。
</p>
一个软考高级考生的摸鱼解压项目。
备考学到头秃的时候，用AI写代码换换脑子，顺便了解一下现在的AI到底有多能打。
整个项目纯AI生成，我只负责提需求和复制粘贴。
考完试先躺平几天，要是不幸挂科了，就回来继续用AI写代码泄愤。
<p align="center">
  <a href="README.md">简体中文</a> · <a href="README.en.md">English</a>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-green.svg" /></a>
  <a href="https://github.com/hl9565/CodexPilot/releases"><img alt="Release" src="https://img.shields.io/github/v/release/hl9565/CodexPilot?label=release" /></a>
  <a href="https://github.com/hl9565/CodexPilot/actions/workflows/release-assets.yml"><img alt="Release assets" src="https://github.com/hl9565/CodexPilot/actions/workflows/release-assets.yml/badge.svg" /></a>
  <a href="https://tauri.app/"><img alt="Tauri" src="https://img.shields.io/badge/Tauri-2.x-24C8DB" /></a>
  <a href="Cargo.toml"><img alt="Rust workspace" src="https://img.shields.io/badge/Rust-workspace-b7410e" /></a>
</p>

CodexPilot 适合已经在本机使用 Codex App 的用户。它提供一个本地管理界面，用 Chromium DevTools Protocol 连接正在运行的 Codex 页面。你可以从这里启动 Codex、导出会话、处理回收站、同步 Provider 归属、配置混合中转和查看诊断日志；它不修改 Codex App 安装目录，也不替换 Codex 本身。

> CodexPilot 是非官方工具，不隶属于 OpenAI 或 Codex App。

![CodexPilot 管理器总览](docs/images/readme-manager-overview.png)

## 快速使用

1. 打开 [GitHub Releases](https://github.com/hl9565/CodexPilot/releases)，在 Assets 区下载对应平台安装包，不要下载 Source code 压缩包。
   - Windows：下载 `CodexPilot-*-windows-x64-setup.exe`，运行安装程序。
   - macOS Apple Silicon：如果该版本提供 `CodexPilot-*-macos-arm64.dmg`，打开后把 `CodexPilot.app` 拖入 Applications。
2. 打开 CodexPilot 管理器，进入“启动”，确认 Codex 路径后点击“启动”。
3. Codex 页面打开后，可以直接使用 CodexPilot 菜单导出当前会话。
4. 需要自定义模型请求时，进入“模型通道”配置混合中转。
5. 需要整理历史会话时，进入“对话维护”处理回收站或同步 Provider 归属。

macOS 当前包未做 Apple Developer ID 签名和公证。如果系统提示无法验证开发者，请先阅读 DMG 内说明，再按需使用随包提供的修复脚本。macOS Intel 当前没有已验证安装包，需要自行从源码验证。

## 核心亮点

### 混合中转

混合中转是 CodexPilot 的重点能力。它保留官方 Codex/ChatGPT 登录态，同时把模型请求切到自定义兼容 API。这样你可以继续使用手机 ChatGPT 控制或接续桌面 Codex，又能让桌面 Codex 的模型请求走自己的中转站。

自定义 Provider 会收到模型请求，隐私、计费和数据处理策略以你配置的 Provider 为准。

![CodexPilot 模型通道页面](docs/images/readme-provider.png)

### Provider 归属同步

切换模型通道后，历史会话可能因为 Provider 元数据不一致而不可见或分组异常。CodexPilot 不会自动改写历史数据；你可以在“对话维护”里先预览影响范围，再手动把会话归属同步到选定 Provider。

![CodexPilot 对话维护页面](docs/images/readme-dialog-maintenance.png)

## 其他功能

- 启动与注入
- 会话导出
- Timeline
- 对话维护
- 归档会话处理
- 诊断快照

完整功能说明见 [docs/features.md](docs/features.md)。

## 本地数据与安全

CodexPilot 会读取或写入本机 `~/.codex` 下的配置、会话、归档会话、状态数据库和备份目录。中转配置档会保存在本机，API Key 不会显示在状态面板里，但仍会写入本地配置文件。

请只在可信设备上使用，并避免把本地配置、日志、截图或备份目录上传到公开仓库。使用自定义兼容 API 时，请自行确认服务提供方的隐私、计费和数据处理策略。

更完整的数据范围见 [功能说明](docs/features.md#本地数据与安全)。

## 文档

- [功能说明](docs/features.md)：启动、模型通道、会话维护、Provider 同步、诊断和本地数据说明。
- [README 维护准则](docs/development/readme-guidelines.md)：项目首页的信息架构和文案规则。

## 交流与支持

如需交流使用问题、反馈异常或获取发布信息，可以加入微信交流群。

<img width="313" height="481" alt="CodexPilot 微信交流群二维码" src="https://github.com/user-attachments/assets/ca69b9b2-64f9-461d-b81b-7f1a3b0eb6b9" />

本项目链接并认可 [LINUX DO](https://linux.do/) 社区。欢迎在社区讨论帖中反馈问题、分享使用体验或提出改进建议。

## License

MIT

