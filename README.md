<p align="center">
  <img src="apps/codex-pilot-manager/src-tauri/icons/icon.png" width="96" height="96" alt="CodexPilot icon" />
</p>

<h1 align="center">CodexPilot</h1>

<p align="center">
  Codex 的本地启动、对话维护与模型通道管理器。
</p>

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

CodexPilot 通过本机管理界面启动和注入 Codex，让会话导出、回收站、Provider 归属同步、混合中转和诊断都变得可控。它不修改 Codex App 安装目录。

> CodexPilot 是非官方工具，不隶属于 OpenAI 或 Codex App。

![CodexPilot 管理器总览](docs/images/readme-manager-overview.png)

## 快速使用

1. 从 [GitHub Releases](https://github.com/hl9565/CodexPilot/releases) 下载安装包。
2. 打开 CodexPilot 管理器。
3. 进入“启动”，确认 Codex 路径和端口状态。
4. 点击“启动”或“重新注入”，从 CodexPilot 打开 Codex。
5. 如需自定义模型通道，进入“模型通道”配置混合中转；如需整理历史会话，进入“对话维护”操作。

macOS 当前包未做 Apple Developer ID 签名和公证。如果系统提示无法验证开发者，请先阅读 DMG 内说明，再按需使用随包提供的修复脚本。

## 核心功能

- **启动与注入**：从桌面管理器启动 Codex，并在 Codex 页面注入 CodexPilot 操作菜单。
- **会话导出**：把当前会话导出为 Markdown，方便归档、检索和分享。
- **对话维护**：删除会话、短时撤销、查看回收站、恢复或永久清理删除备份。
- **归档会话处理**：支持归档会话的导出、删除和批量删除。
- **混合中转**：保留官方 Codex/ChatGPT 登录态，同时把模型请求切到自定义兼容 API。
- **Provider 归属同步**：手动预览并同步历史会话的 Provider 元数据，避免自动改写本地历史数据。
- **诊断快照**：收集启动、注入、页面连接、路由和中转配置相关日志，便于定位问题。

完整功能说明见 [docs/features.md](docs/features.md)。

## 安装

从 [GitHub Releases](https://github.com/hl9565/CodexPilot/releases) 下载对应平台安装包：

- Windows：`CodexPilot-*-windows-x64-setup.exe`
- macOS Apple Silicon：`CodexPilot-*-macos-arm64.dmg`（如该版本提供）

Windows 运行安装程序后会创建桌面和开始菜单快捷方式。

macOS 如提供 DMG，打开后把 `CodexPilot.app` 拖入 Applications。macOS Intel 构建脚本预留了 `x86_64-apple-darwin` target，但当前未作为已验证安装包发布；如果你使用 Intel Mac，需要自行从源码验证打包。

### 源码运行

从源码运行需要先安装 Rust、Node.js 和 npm：

```bash
cd apps/codex-pilot-manager
npm install
npm run dev
```

源码运行适合本地调试和临时使用，不需要先打包成 DMG。

## 本地数据与安全

CodexPilot 会读取或写入本机 `~/.codex` 下的配置、会话、归档会话、状态数据库和备份目录。中转配置档会保存在本机，API Key 不会显示在状态面板里，但仍会写入本地配置文件。

请只在可信设备上使用，并避免把本地配置、日志、截图或备份目录上传到公开仓库。使用自定义兼容 API 时，请自行确认服务提供方的隐私、计费和数据处理策略。

更完整的数据范围见 [功能说明](docs/features.md#本地数据与安全)。

## 文档

- [功能说明](docs/features.md)：启动、模型通道、会话维护、Provider 同步、诊断和本地数据说明。
- [架构说明](docs/development/architecture.md)：项目结构和主要模块。
- [README 维护准则](docs/development/readme-guidelines.md)：项目首页的信息架构和文案规则。
- [发布流程](docs/development/release.md)：打包、发布和发布前检查。
- [路线图](docs/development/roadmap.md)：后续方向。

## 交流与支持

如需交流使用问题、反馈异常或获取发布信息，可以加入微信交流群。

<img width="313" height="481" alt="CodexPilot 微信交流群二维码" src="https://github.com/user-attachments/assets/ca69b9b2-64f9-461d-b81b-7f1a3b0eb6b9" />

本项目链接并认可 [LINUX DO](https://linux.do/) 社区。欢迎在社区讨论帖中反馈问题、分享使用体验或提出改进建议。

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

然后打开 `http://127.0.0.1:1420`。预览模式会使用本地 mock 数据，覆盖启动、模型通道、对话维护和诊断页面；外层窗口默认使用真实 App 配置里的 `1120x760` 尺寸，方便检查 UI 在实际桌面窗口中的表现。

## License

MIT
