# CodexLink

[English](README.md) | [简体中文](README.zh-CN.md)

**CodexLink 让 ChatGPT、Claude 等在线 AI 直接使用你电脑上真实的项目——代码始终留在你自己的机器里。**

你可以让 AI 读取仓库、做受保护的修改、跑测试、排查失败。这些操作由本机 Runner 在你明确注册的项目目录内执行，返回给模型的是工具结果，而不是你的文件系统。不需要把项目上传到任何托管工作区，也不需要开放任何入站端口。

CodexLink 构建在 [WebCodex](#引擎与许可) 引擎（Apache-2.0）之上。产品层——Windows 桌面安装器、项目与权限管理、运行时生命周期、默认安全执行策略、诊断与发布纪律——属于 CodexLink；引擎与运行时契约从上游选择性同步。

## 工作方式

```text
AI 客户端（ChatGPT / Claude / 自定义 GPT）
   |
   |  MCP / GPT Actions / REST
   v
CodexLink Server        鉴权 · 策略 · 审计 · 会话账本
   |
   |  QUIC / WebSocket / 轮询（由 Runner 主动外连）
   v
你机器上的 Runner
   |
   +-- 已注册的项目目录
   +-- Git
   +-- 编译器 / 测试 / 工具链
```

有两条前提值得先知道：

- **Server 从不扫描你的磁盘，也从不读取项目文件。** 项目由 Runner 注册上来，Server 只用一个运行时项目 ID `agent:<client_id>:<project_id>` 来寻址。
- **Runner 永远向外连接。** 不需要公网 IP、端口映射或入站防火墙规则，NAT 后面和纯内网环境都能用。

完整的 Server/Runner 架构、协议接口与权限边界见[架构说明](docs/ARCHITECTURE.md)、[MCP](docs/MCP.zh-CN.md)和[认证模型](docs/AUTH_MODEL.zh-CN.md)。

## 能做什么

- **理解并修改代码** —— 在配置好的项目范围内读取、搜索、分析，并做受保护的修改。
- **使用真实开发环境** —— 在仓库所在机器上运行命令、测试、格式化、编译器和项目自己的工具。
- **检查 Git** —— 查看状态与差异，让每一次代码变化保持可见、可审查。
- **处理长时间任务** —— 任务持续运行并可观察、可恢复，不需要一次模型回复一直等待到底。
- **保留人工审查** —— 通过运行时控制台与任务流程进行指导、取消、接受或拒绝。

## 默认安全

CodexLink 托管的 Runner 默认进入**安全执行模式**。在总执行闸门关闭的状态下，不会向模型暴露原始 shell、任意进程或脚本执行、分离进程、持久 shell、SSH 执行、验证执行以及子代理进程；类型化的文件、补丁与 LSP 操作仍然可用。

需要说清楚边界：这是**策略基线，不是操作系统沙箱**。在 Windows 普通用户令牌下运行的进程，其文件系统权限仍可能超出逻辑项目根，除非由操作系统强制限制。请把 `allowed_root` 设成具体的项目目录，而不是盘符根目录，完整模型见 [SECURITY.md](SECURITY.md)。

## 安装与连接

Windows 上推荐 **CodexLink Desktop + 官方 OpenAI Secure MCP Tunnel**：

1. 运行 CodexLink 安装器（Windows x64，当前用户安装）。
2. 添加要让 AI 操作的项目，注册路径尽量收窄。
3. 在应用内创建隧道与运行时凭据。
4. 在 ChatGPT 中添加 MCP 连接。

具体步骤见 [Desktop 安装与连接指南](docs/desktop-install.zh-CN.md)；项目、连接与后台运行见 [Desktop 日常使用](docs/desktop-guide.zh-CN.md)。

## 版本说明

CodexLink 会同时显示两个版本，二者刻意分开：

- **CodexLink 产品版本** —— 你安装的那一份（安装器、激活、桌面外壳）。
- **WebCodex 引擎版本** —— 该构建所基于的运行时基线（当前 `0.4.1`）。

关于页与诊断信息会同时展示两者。每个发布记录会把产品版本、源码提交、引擎版本、隧道客户端构建和安装包哈希固定在一起。

## 平台支持

- **Windows x64** —— CodexLink 桌面产品：本机 Server + Runner、官方 OpenAI Secure Tunnel、当前用户安装器。
- **Windows arm64** —— 引擎级 CLI + Runner、本地前台 Server 与 `share`；桌面安装器目前仅 x64。
- **Linux x64/arm64** —— 引擎级 `share`、Server 与 Runner 工作流。
- **macOS x64/arm64** —— 引擎级 Desktop 本机 Server + Runner、OpenAI Secure Tunnel、`share` 与独立 Runner 工作流。

## 引擎级试用

想先快速看一眼效果，可以用引擎 CLI 起一个临时的单项目受限环境。这一条走的是上游 npm 包，不是 CodexLink 产品：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

`share` 会输出连接信息，命令退出后即失效。它适合试用与临时分享，不是日常使用方式。

## 文档

以下文档内容完整且当前有效，但正文仍在从引擎名逐步迁到 CodexLink 产品名，文件名与 CLI 名称沿用引擎命名。

- [Desktop 安装与连接](docs/desktop-install.zh-CN.md) —— 推荐路径：Desktop + 官方 OpenAI Secure Tunnel
- [Desktop 日常使用](docs/desktop-guide.zh-CN.md) —— 项目、连接、活动与后台运行
- [完整使用指南](docs/PERSONAL_SETUP.zh-CN.md) —— CLI、已有 Server、自托管与高级 Server + Runner 配置
- [快速试用](docs/QUICK_START.zh-CN.md) —— 用 `share` 临时体验一个仓库
- [MCP](docs/MCP.zh-CN.md) —— ChatGPT、Claude、认证方式与 MCP 参考
- [部署指南](docs/DEPLOYMENT.zh-CN.md) —— 生产、自托管、systemd/Docker、OAuth 与代理
- [故障排查](docs/TROUBLESHOOTING.zh-CN.md) —— 连接与运行问题
- [CLI](docs/CLI.zh-CN.md) —— 命令与凭据参考
- [安全说明](SECURITY.md) —— 安全模型与使用建议
- [交付路线图](docs/CODEXLINK_DELIVERY_ROADMAP.md) —— 产品分层、安全闸门与发布策略
- [文档索引](docs/INDEX.zh-CN.md) —— 全部用户与贡献者文档

## 从源码构建

服务端与引擎二进制：

```bash
cargo build --release --workspace --bins
```

桌面应用（Tauri + React）：

```bash
cd apps/desktop
npm ci
npm run tauri build
```

## 参与贡献

欢迎提交贡献，也欢迎使用 CodexLink 本身或其他 coding agent 辅助开发。开发流程、评审要求与仓库规则见 [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md) 与 [AGENTS.md](AGENTS.md)。

## 引擎与许可

本仓库是基于 **WebCodex** 项目的修改版本，按 Apache License 2.0 分发。上游版权声明与许可证原文保留在 [LICENSE](LICENSE) 中；面向 CodexLink 的改动记录在提交历史与[交付路线图](docs/CODEXLINK_DELIVERY_ROADMAP.md)里。

CodexLink 是独立的产品名称与产品身份。引擎保留自己的名称、版本线、CLI 与文档，以便上游同步始终可追溯。

## 致谢

感谢 WebCodex 的维护者与贡献者——这套引擎是本产品得以存在的基础；也感谢 [LINUX DO](https://linux.do/) 社区提供友好的技术交流与开源分享环境。
