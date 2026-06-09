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

CodexPilot 适合已经在本机使用 Codex App 的用户。它提供一个本地管理界面，用 Chromium DevTools Protocol 连接正在运行的 Codex 页面。你可以从这里启动 Codex、解锁插件入口、导出会话、处理回收站、执行对话同步和查看诊断日志；它不修改 Codex App 安装目录，也不替换 Codex 本身。

> CodexPilot 是非官方工具，不隶属于 OpenAI 或 Codex App。

![CodexPilot 管理器总览](docs/images/readme-manager-overview.png)

## 快速使用

1. 打开 [GitHub Releases](https://github.com/hl9565/CodexPilot/releases)，在 Assets 区下载对应平台安装包，不要下载 Source code 压缩包。
   - Windows：下载 `CodexPilot-*-windows-x64-setup.exe`，运行安装程序。
   - macOS Apple Silicon：如果该版本提供 `CodexPilot-*-macos-arm64.dmg`，打开后把 `CodexPilot.app` 拖入 Applications。
2. 打开 CodexPilot 管理器，进入“启动”，确认 Codex 路径后点击“启动”。
3. Codex 页面打开后，可以直接使用 CodexPilot 菜单导出当前会话，也可以用 Pilot 浮窗里的 Fast 开关控制服务档位。
4. 需要整理历史会话时，进入“对话维护”处理回收站或执行对话同步。

macOS 当前包未做 Apple Developer ID 签名和公证。如果系统提示无法验证开发者，请先阅读 DMG 内说明，再按需使用随包提供的修复脚本。macOS Intel 当前没有已验证安装包，需要自行从源码验证。

## 核心亮点

### 免登录插件解锁

启动页的“页面增强”里提供“插件入口解锁”和“特殊插件强制安装”开关。未登录 ChatGPT、只使用 API Key 模式时，CodexPilot 可以解锁 Codex 原生插件入口，并解除部分插件因 `App unavailable` / `应用不可用` 导致的安装按钮禁用。

这个能力只作用于当前运行中的 Codex 页面，不接管 Provider 切换，也不要求你把 `~/.codex/config.toml` 改成 CodexPilot 自己的配置流。

![CodexPilot 页面增强与插件解锁](docs/images/readme-launch.png)

解锁成功后，Codex 原生侧栏会直接显示“插件 - 已解锁”。

![CodexPilot 插件已解锁侧栏效果](docs/images/readme-plugin-unlocked-snippet.png)

### 全局 Fast 与单对话 Fast

启动页的“页面增强”里提供“全局 Fast”开关，默认开启。开启后，CodexPilot 会把所有对话请求改写为 Fast（priority）服务档位，适合希望默认降低等待时间的工作流。

![CodexPilot 全局 Fast 浮窗状态](docs/images/readme-fast-global.png)

关闭“全局 Fast”后，可以在 Codex 页面右下角的 Pilot 浮窗里用闪电按钮按当前对话切换 Fast / Standard；在新对话页面提前打开 Fast 时，下一条新对话会从第一条消息开始使用 Fast。成本明细里也会显示当前服务档位和倍率，方便确认请求是否走到了 Fast。

![CodexPilot Fast 成本明细](docs/images/readme-fast-cost-detail.png)

如果通过 sub2api 网关使用 CodexPilot，需要同时在 sub2api 的“系统设置” - “网关服务”里开启 OpenAI Fast/Flex 策略：把 `service_tier` 匹配设为 `priority（fast）`，处理方式设为“透传（保留 service_tier）”，这样 CodexPilot 写入的 Fast 参数才能继续传到 OpenAI 网关。

![sub2api 网关服务 Fast/Flex 策略](docs/images/readme-sub2api-fast-gateway.png)

### 对话同步

当 ccSwitch 或其他工具切换 `~/.codex/config.toml` 里的 `model_provider` 后，历史会话可能因为 Provider 元数据不一致而不可见或分组异常。CodexPilot 不会自动改写历史数据；你可以在“对话维护”里先预览影响范围，再手动把会话归属同步到当前配置或手动指定的 Provider。

![CodexPilot 对话维护页面](docs/images/readme-dialog-maintenance.png)

## 其他功能

- 启动
- 页面增强开关
- 全局 Fast 与单对话 Fast
- 会话导出
- Timeline
- 对话维护
- 归档会话处理
- 诊断快照

完整功能说明见 [docs/features.md](docs/features.md)。

## 本地数据与安全

CodexPilot 会读取本机 `~/.codex/config.toml` 的当前 Provider，并读写本机 `~/.codex` 下的会话、归档会话、状态数据库和备份目录。

请只在可信设备上使用，并避免把本地配置、日志、截图或备份目录上传到公开仓库。模型 Provider 切换和 API Key 管理请交给 ccSwitch 或你自己的 Codex 配置流程。

更完整的数据范围见 [功能说明](docs/features.md#本地数据与安全)。

## 文档

- [功能说明](docs/features.md)：启动、会话维护、对话同步、诊断和本地数据说明。
- [README 维护准则](docs/development/readme-guidelines.md)：项目首页的信息架构和文案规则。


本项目链接并认可 [LINUX DO](https://linux.do/) 社区。欢迎在社区讨论帖中反馈问题、分享使用体验或提出改进建议。

## License

MIT
