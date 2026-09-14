# CodexLink Delivery Roadmap

> 状态：**2026-09-14 首个商业版本的统一执行路线图，安全方向已正式冻结。**
>
> 目标：把 CodexLink 从 WebCodex 0.4.1 Engine 基线恢复成可商业交付、可维护、可回滚、可长期跟随上游 Engine 的 Windows 产品。
>
> 原则：保持主线简单、真实、可验证；不为“看起来更安全”堆叠复杂机制。正常开发能力尽量不打扰，真正高破坏性的边界由 typed contract 与 Destructive Guard 硬控制。
>
> 首版详细安全规范见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。

## 0. 2026-09-14 当前已核实状态

### Engine / 开发链路

- WebCodex Engine 基线：`0.4.1`，source `26c44e6436f254183d74db6865d04e2ec1d85930`。
- Server / Runner source alignment 已对齐。
- Windows Runner / WebCodex 开发链路当前可用。
- 工作目录：`D:\GitHub\CodexLink`。
- 分支：`codexlink-rebuild`。
- 旧 RC / test installation / VPS 继续作为受保护证据，不在本轮文档整理中修改。

### 已完成 checkpoint

- `22cab846` — Safe Mode / Engine baseline；
- `863d5396` — Product identity；
- `b191bd85` — bounded state migration；
- `733b7b54` — RC5 device activation；
- `e85749d7` — secure Tunnel handoff（已推远端）；
- `7ebab38d` — Windows installer lifecycle（本地完成，待 GitHub TLS 恢复后普通 push）；
- `c5fa9b50` — Windows Sandbox isolation POC（**研究实验，不是产品方向**）。

### 当前正式产品决策

Windows Sandbox POC 已完成其研究价值，但首版不采用 OS sandbox 作为发布依赖：

- 不要求用户开启 Windows Optional Feature；
- 不要求安装后为 sandbox 重启系统；
- 不把 POC 接入 Desktop / Runner production path；
- 后续用普通 revert 保留历史记录并把 POC 从产品主线移除，不 reset / 不改写历史；
- 下一阶段改为 **Batch F — Destructive Guard & Bounded Recursive Delete**。

在 Batch F hard gate 通过之前，当前 Safe Mode 继续保持 model-authored arbitrary execution 关闭；Guard 通过后再恢复正常 shell / process / script execution。

### Git / 远端原则

- 不 force-push；
- 不 reset published history；
- GitHub TLS 故障未恢复时不使用关闭 TLS 校验等绕过；
- `.codexlink-c087.diff` 继续作为历史证据保留，不 broad clean。

### 历史 stable-v1 源码状态

`D:\GitHub\CodexLink-stable-v1` 当前不能再作为完整 Git 源码树使用；只剩不完整目录结构，不能把它当成恢复产品功能的唯一来源，也不要再向其中写入内容。

历史功能恢复时的可信证据顺序：

1. 已验收 RC 安装包及其 release metadata / SHA256；
2. 已存在的私有 Git 历史、已提交 patch 和测试；
3. 已安装的真实 CodexLink 行为与持久数据结构；
4. 已核实的产品文档和验收记录；
5. 最后才根据需要重新实现缺失部分。

### 当前接受的历史 RC 证据

`D:\Test\CodexLink\Current-RC` 中的 RC5 必须保持原样：

- CodexLink `1.0.0-rc.5`
- 状态：`UNSIGNED TEST ONLY`
- 历史产品源码：`0f63d08ed91c085584ebea2d3a8413d2b79af10f`
- WebCodex Engine：`0.4.1`
- Engine source：`26c44e6436f254183d74db6865d04e2ec1d85930`
- Installer SHA256：`A7CB52ADE7000F8A82CEB29367828B56A0F6916806752F860C02364262E4756C`

RC5 是行为和包装参考，不是当前 `codexlink-rebuild` 源码的可直接继续开发 worktree。

---

## 1. 产品边界

CodexLink 的长期结构固定为：

```text
CodexLink Product Shell
  ├─ 品牌 / UI / UX
  ├─ License / Activation
  ├─ Installer / Upgrade / Uninstall
  ├─ Diagnostics / Support
  ├─ Project / Connection workflow
  └─ Commercial website / tutorials
            ↓
CodexLink Compatibility / Safety Adapter
  ├─ execution policy / Destructive Guard
  ├─ product-specific state / migration
  ├─ bundled tunnel-client routing
  └─ upstream compatibility shims
            ↓
WebCodex Engine
  ├─ Server
  ├─ Runner
  ├─ MCP / runtime tool contracts
  ├─ Jobs / Workflow Sessions
  ├─ structured file / Git / LSP tools
  └─ Windows correctness / security fixes
```

产品策略：

- 客户看到的是 CodexLink 产品版本，不跟随上游 WebCodex Desktop UI。
- 上游主要跟随 Engine、安全、协议和 Windows correctness，不盲目合并上游 Desktop frontend。
- About / Diagnostics 中分别显示 CodexLink Product version、WebCodex Engine version、Tunnel Client version。
- 开源归属放在 About / Third-party notices，客户主流程不暴露不必要的实现细节。

---

## 2. 非协商安全基线

首版详细规范以 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md) 为准；本路线图只保留不会轻易变化的产品原则。

### 2.1 正常 execution 是首版能力，但必须晚于 Guard

产品目标允许正常开发 execution：

- shell；
- process；
- script；
- Cargo / npm / Python / Node / PowerShell 等项目工作流。

当前代码里的 Safe Mode 是**过渡期硬门**：在 Batch F 的 destructive admission、typed delete 和 Windows regressions 全部通过前，不先行开放 arbitrary execution。

### 2.2 Destructive Guard，而不是 approval 地狱

首版不做“每个 shell 都问用户”。策略是：

- 正常开发操作直接允许；
- 一个明确文件的删除允许；
- 一个边界可证明的明确目录允许 bounded recursive delete；
- root / parent / wildcard / protected anchor / reparse escape 拒绝；
- raw recursive delete、destructive Git、mirror/purge、disk destructive primitive 在 spawn 前拒绝；
- 被拒绝的高风险操作第一版不提供“继续执行/关闭保护”按钮，用户需要时在本机 Terminal 手动完成。

### 2.3 Bounded Recursive Delete

不一刀切禁止递归删除。

`delete_directory(path)` 可以递归清理 `node_modules`、`target`、`dist`、深层 cache 或明确的旧源码子树，但必须满足：

- target 是一个明确目录；
- target 是授权 root 的严格后代；
- target 不是 drive/filesystem root、授权 root、parent 或 protected anchor；
- 不接受 wildcard；
- preflight 只做轻量路径/边界检查；
- 递归只走一遍目录树；
- 任何 symlink / junction / reparse point 都不 follow，并 fail closed；
- 完成后返回紧凑删除摘要，不刷数万条文件记录。

### 2.4 Project authorization 不是 OS sandbox

`allowed_root` 表达的是 CodexLink / Runner 逻辑授权，不会收回当前 Windows 用户 token 的其他权限。

因此首版：

- 不宣传恶意代码 containment；
- 不承诺 Python/Node 脚本内部故意调用 destructive API 时一定被 Guard 拦截；
- Windows Sandbox / Restricted Token / AppContainer 均不是首版 release Gate；
- 以后只有真实用户需求证明有必要时，再把强隔离作为独立可选能力评估。

---

## 3. 总优先级

| 优先级 | 工作 | 当前状态 | 完成门槛 |
| --- | --- | --- | --- |
| P0 | Engine / 开发链路基线 | **已完成** | Server + Runner 0.4.1 source aligned；安全 baseline 有 checkpoint |
| P0 | Product identity / state / migration / license / Tunnel | **已完成** | Batch A-D focused tests 已通过 |
| P0 | Lifecycle / installer | **本地完成** | `7ebab38d`；待 GitHub TLS 恢复后 push，真实 upgrade 仍留给新 RC |
| P0 | Batch F：Destructive Guard & Bounded Delete | **下一阶段** | typed 增删改读 + bounded recursive delete + destructive admission + focused Windows regressions |
| P0 | 恢复正常 model execution | **等待 Batch F** | Guard 先全绿，再开放 shell/process/script；不做 OS sandbox Gate |
| P0 | Windows 新 RC 端到端验收 | 待做 | install → activate → project → tunnel → ChatGPT → read/edit/create/delete/shell → guard → restart/upgrade/uninstall |
| P1 | 商业网站 + 小白图文教程 | 待做 | 真实截图、当前 OpenAI 地址、版本历史、下载/支持、安全边界说明上线 |
| P1 | 正式商业发布准备 | 待做 | 签名 installer、SHA、归档、第三方许可、发布记录齐全 |
| P2 | CodexLink UI V2 / 更强产品差异化 | 后置 | 不阻塞首个稳定商业版本 |

---

## 4. Stage 0 — 开发与备份基础设施

### 已完成

- 私有 GitHub 仓库已存在；
- `codexlink-rebuild` 已保存安全基线 commit；
- Tunnel 开发路径已恢复；
- Server / Runner 0.4.1 已对齐。

### 立即收尾

- [ ] 审阅 `.codexlink-c087.diff`，确认它只是安全 patch 备份还是还有独有内容；在未确认前不删除。
- [ ] 把本路线图和 recovery matrix 做成一个独立文档 checkpoint。
- [ ] 明确 `main` 与 `codexlink-rebuild` 的合流方式，不 force、不 reset published history。
- [ ] 为每个后续阶段保持“小改动 → focused test → Windows 验证（如适用）→ diff review → commit → remote backup”的节奏。

### Gate

任何新的产品重建工作开始前，必须可以回答：

1. 当前 source HEAD 是什么；
2. 当前 dirty changes 是什么；
3. 发生故障时回退到哪个 commit / artifact；
4. 这个 checkpoint 是否已经存在于另一台/远端存储中。

---

## 5. Stage 1 — “旧 RC5 → 新 rebuild”恢复矩阵 — DONE / 持续维护

恢复矩阵已经建立，并已随 Batch A-E 的真实实现更新。它现在是持续维护的状态表，不再是“开始产品恢复前”的待办。历史能力仍按 RC 行为和当前上游 0.4.1 源码交叉确认，避免凭记忆重写。

恢复顺序：

1. Product identity / namespace；
2. persistent state / migration；
3. License / Activation；
4. project authority；
5. OpenAI Tunnel packaging / configuration；
6. Desktop lifecycle / single-instance / autostart；
7. installer / upgrade / uninstall；
8. diagnostics / version split；
9. customer copy / localization；
10. RC packaging / archive metadata。

详细项见 `docs/CODEXLINK_RECOVERY_MATRIX.md`。

### Gate

每个恢复项必须标注：

- 历史证据来源；
- 当前 rebuild 状态；
- 实现文件；
- focused tests；
- Windows 验证方法；
- 是否会影响升级/回滚。

---

## 6. Stage 2 — CodexLink 产品核心恢复 — A-D DONE / E LOCAL DONE

Batch A-D 已完成并形成 checkpoint；Batch E lifecycle / installer 已本地完成，待 GitHub TLS 恢复后普通 push，真实 upgrade 留给新 RC。

### 6.1 Product identity

已恢复并验证：

- `CodexLink` 品牌；
- product / bundle identifier；
- Windows installer 名称和默认安装目录；
- `dev.codexlink.desktop` 持久数据 namespace；
- WebCodex 仅作为 Engine / attribution 出现在适当位置。

### 6.2 程序目录与持久数据分离

固定模型：

```text
Program install dir
  ≠
Persistent user-data dir
```

持久数据默认：

```text
%LOCALAPPDATA%\dev.codexlink.desktop
```

这里承载：

- desktop state；
- license；
- encrypted/secret state；
- runtime；
- connections。

升级或卸载程序文件时不得顺便删除用户激活、项目、Tunnel 配置等持久状态。

### 6.3 Legacy migration

历史迁移继续遵循 copy-only + bounded migration：

- 只复制明确的已知状态；
- 不删除旧目录；
- 只 rebase CodexLink-owned internal absolute paths；
- 不修改用户项目路径 / allowed roots；
- migration marker 阻止用户已经删除的新状态被旧目录再次“复活”。

### 6.4 License / Activation

已恢复商业激活流程，同时保留 RC5 已接受的简化 customer copy：

- 页面只显示客户需要知道的激活信息；
- 继续保留 `MYWS-...` prefix，除非以后单独做产品决策；
- 不在普通 UI 暴露 Ed25519、offline verification、明文/内部存储实现等细节；
- 私有签名 key 永不进入客户端；
- 本地验证和错误路径 fail closed；
- 技术细节只进入受控 support diagnostics。

### 6.5 Project authority

- 用户选择哪个目录，就 canonicalize 并授权那个精确 root；
- 不偷偷扩大到父目录；
- 仍保留底层 traversal / symlink / junction / sensitive-target 防护；
- 整盘 root 作为显式高级选择时，不等于开启 arbitrary shell。

### 6.6 Tunnel client packaging

商业 Windows 包使用固定且验证 SHA 的 OpenAI tunnel-client，不回退到旧 `WebCodex\tools` cache。

历史 RC5 固定 client：

- tunnel-client version：`0.0.12`
- SHA256：`6649169733686805CA16CCCD91774594D0C017FD729C37AD4CE1CD18323D9AE8`

目标路径归 CodexLink 安装目录所有，例如：

```text
<CodexLink install>\webcodex-runtime\tools\tunnel-client.exe
```

缺失或 SHA 不匹配必须 fail closed。

### 6.7 Desktop lifecycle

已本地恢复：

- single-instance；
- system tray；
- 登录时启动；
- `runtime_autostart` desired-state；
- 停止/退出只处理 CodexLink-owned process tree，不按进程名广泛杀其他实例。

### Gate

产品核心恢复 Gate 已基本满足；Batch E 只剩 remote backup 与真实 RC upgrade 证据。新的视觉大改版继续后置，不阻塞 Batch F、RC 和商业发布。

---

## 7. Stage 3 — Batch F：Destructive Guard & Bounded Recursive Delete

详细规范：[`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。

### 实现顺序

1. typed `create_file` 的 no-overwrite contract；
2. guarded `edit_file` / patch 的 expected-SHA contract；
3. typed `rename_file` no-overwrite；
4. typed `delete_file`；
5. typed `delete_directory` + lightweight preflight；
6. single-pass bounded recursive delete；
7. reparse/junction/symlink fail-closed；
8. protected root / parent / anchor guard；
9. raw-shell destructive admission；
10. bounded deletion summary / safe audit event；
11. short model safety instructions；
12. focused Windows regression；
13. **最后才把 execution policy 从临时 Safe Mode 切换为 normal execution + Destructive Guard。**

### 明确不做

- 不做 OS sandbox；
- 不做 Windows Sandbox Optional Feature 产品依赖；
- 不做 interpreter 逐项 approval；
- 不做“危险命令百科全书”或完整 shell parser；
- 不做文件数量/体积固定阈值；
- 不要求 Git 才能安全删除；
- 不做 destructive bypass 的“仍然执行”按钮。

### Gate

Batch F 未通过前，保持当前 arbitrary execution deny。

Batch F 通过必须证明：正常 build/test/script 能运行，而 obvious recursive delete、root/parent destructive target、destructive Git、mirror/purge、disk destructive primitive 在 child spawn 前被拒绝；typed recursive delete 能安全处理深目录和大型 generated tree，并且不 follow reparse point。

---

## 8. Stage 4 — Windows 真实机端到端验收

每个候选必须在真实 Windows 上验证，不只跑单元测试。

### 最小全链

1. clean/fresh install；
2. custom install path；
3. activation；
4. 选择真实项目；
5. Server / Runner ready；
6. 配置 Tunnel ID / Runtime Key；
7. 启动 Secure Link；
8. ChatGPT Developer Mode / App 连接；
9. typed read / search；
10. typed create + guarded edit + Git diff；
11. single-file delete；
12. bounded recursive `delete_directory`（使用 disposable test tree）；
13. normal shell / build / test；
14. destructive raw shell 被 Guard 拒绝且没有 child spawn；
15. project root / parent / reparse destructive regression；
16. 重启 CodexLink；
17. Windows 登录 autostart（启用时）；
18. upgrade 覆盖安装并复用正确状态；
19. uninstall 后验证持久用户数据策略；
20. reinstall / downgrade（在兼容范围内）。

在本机通过后，再让至少一台**从未部署过 CodexLink 的朋友/测试机**重复 clean-install 主链，专门找安装环境、依赖、权限和首次使用体验问题。

### 必查隔离

- 新机器/干净 profile 不应创建 customer-visible `WebCodex` 产品目录；
- 不触碰机器上独立安装的 upstream WebCodex；
- 不触碰外部 npm Runner；
- 不复用旧 WebCodex tunnel-client cache；
- 没有激活时，不应后台预启动 Server / Runner / Tunnel；
- 第二实例和 `--autostart` 不产生重复 runtime。

---

## 9. Stage 5 — 商业网站与“小白可跟做”教程

网站不是一篇 Tunnel 文档，而是完整产品站。

### 信息架构

- 首页 / 产品定位；
- 购买与下载；
- 使用教程；
- 常见问题 / 故障排查；
- 版本历史；
- 关于 / 开源许可 / Third-party notices；
- 支持与联系方式。

### Quick Start 教程顺序

```text
01 下载与安装
→ 02 激活 CodexLink
→ 03 添加本地项目
→ 04 创建 OpenAI Tunnel
→ 05 创建 Runtime/API Key
→ 06 在 CodexLink 中连接
→ 07 开启 ChatGPT Developer Mode
→ 08 创建 CodexLink App
→ 09 Scan Tools / 首次使用
```

### 截图规则

- 只使用真实 OpenAI / ChatGPT / CodexLink 截图，不使用 AI 生成界面图；
- 一张图只讲一个动作组；
- 用编号、框和箭头指出点击位置；
- Runtime/API key 必须重度打码；
- 不使用真实激活码、个人邮箱、组织名或客户数据；
- 使用专门的 documentation/test account；
- OpenAI 页面和 URL 在网站发布前必须再次实测，不能只依赖旧截图。

当前上游文档可作为 URL 候选证据，例如：

- `https://platform.openai.com/settings/organization/tunnels`
- `https://platform.openai.com/settings/organization/api-keys`

但正式网站上线前仍需在真实账号里确认当前 route、按钮名和权限文案。

### 产品内与网站的职责

CodexLink EXE 中只保留简短稳定入口，例如“查看创建教程”。容易变化的 OpenAI 截图和步骤留在网站，这样 OpenAI UI 改版时不需要重新发 installer。

---

## 10. Stage 6 — RC 与正式商业发布

### RC 不可覆盖

每个候选必须唯一：

```text
Current-RC/
Archive/<version>/
Rejected/<version>/
```

接受的 installer 永不覆盖，rejected 名称永不复用。

每个 RC 记录：

- Product version；
- Product source commit；
- Engine version + commit；
- Tunnel Client version + SHA；
- Installer SHA256；
- build/signing status；
- validation evidence；
- known issues；
- recommended / rejected status。

### 正式发布策略

CodexLink 当前商业策略：

- 不做自动更新；
- 不做 in-product 网络“检查更新”；
- 不依赖 Tauri updater；
- 不依赖 CDN / object storage；
- 安装包人工上传到百度网盘 / 夸克网盘；
- QQ / 支持渠道通知；
- 用户自己选择升级/降级时机；
- 历史版本永久归档；
- 网站版本页记录日期、SHA256、Product/Engine version、changes、known issues、recommended status。

正式商业版还需要：

- Windows code signing；
- installer signature + SHA 校验；
- Third-party / Apache-2.0 notices；
- 最终隐私/安全/支持文案；
- 真实机器 clean-install final acceptance。

---

## 11. Stage 7 — 首发以后再做的 UI V2

首个商业版稳定后，再用真正的 CodexLink 产品心智重做信息架构，例如：

```text
工作台
工作区
AI 接入
运行中心
设置
```

目标不是“把 WebCodex UI 改颜色”，而是围绕客户真正关心的问题：

- AI 现在能不能工作？
- 哪个项目被授权？
- ChatGPT 连接到哪里？
- 刚才发生了什么？
- 下一步该做什么？

先做可交互 HTML/CSS 原型，用户选择方向后再集成 Tauri。这个阶段不阻塞安全、安装、激活、Tunnel、网站和首发。

---

## 12. 当前下一步顺序

从本次安全决策文档 checkpoint 开始，严格按以下顺序推进：

1. **文档收口**：把路线图、recovery matrix 与安全边界规范统一并提交；继续保留 `.codexlink-c087.diff` 历史证据。
2. **移除 Sandbox POC 产品主线**：对 `c5fa9b50` 做普通 revert；保留历史，不 reset / 不重写。
3. **推送本地 checkpoint**：GitHub TLS 恢复后把 `7ebab38d`、POC revert、文档 checkpoint 用普通 push 备份到 `origin/codexlink-rebuild`。
4. **实现 Batch F**：typed 增删改读、Bounded Recursive Delete、lightweight preflight、reparse fail-closed、protected anchors、raw-shell Destructive Guard、safe audit。
5. **Batch F focused gate**：保持 Safe Mode 直到上述安全测试全绿；不得提前恢复 execution。
6. **恢复 normal execution**：允许正常 shell/process/script/build/test，不做 interpreter approval，不做 OS sandbox Gate。
7. **生成新的唯一 RC**：Windows 真机执行完整 install → activate → project → tunnel → ChatGPT → files → execution → guard → restart/upgrade/uninstall 链。
8. **外部 clean-machine 测试**：至少一台从未安装过 CodexLink 的朋友/测试机复跑首次安装和核心链路，记录真实 UX / 环境问题。
9. **修正 RC 反馈并重新候选**：发现问题就修，不覆盖 accepted/rejected 历史 artifact。
10. **完成网站 + 图文教程 + 安全边界说明**。
11. **签名、归档、第三方许可、正式发布**。
12. **首发稳定后再推进 UI V2 / 可选高级安全能力**。

任何阶段发现会扩大 destructive scope、权限、credential exposure、历史覆盖或不可逆迁移时，立即 stop-the-line，先解决边界再继续。
