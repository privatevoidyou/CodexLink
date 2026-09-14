# CodexLink Delivery Roadmap

> 状态：**2026-09-14 首个商业版本的统一执行路线图，安全、UI 与商业激活方向已正式冻结。**
>
> 目标：把 CodexLink 从 WebCodex 0.4.1 Engine 基线恢复成可商业交付、可维护、可回滚、可长期跟随上游 Engine 的 Windows 产品。
>
> 原则：保持主线简单、真实、可验证；不为“看起来更安全”堆叠复杂机制。正常开发能力尽量不打扰，真正高破坏性的边界由 typed contract 与 Destructive Guard 硬控制。
>
> 首版详细安全规范见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。
>
> 首版产品壳与极简配置 UX 见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。
>
> 首版购买资格、下载与设备激活合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

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
- `c5fa9b50` — Windows Sandbox isolation POC（**研究实验，不是产品方向**）；
- `65a97891` — 普通 revert Sandbox POC，保留历史且从产品主线移除；
- Batch F — Destructive Guard & Bounded Recursive Delete（本地 focused Gate 已通过；本路线图随 Batch F checkpoint 固化）。

### 当前正式产品决策

Windows Sandbox POC 已完成其研究价值，但首版不采用 OS sandbox 作为发布依赖：

- 不要求用户开启 Windows Optional Feature；
- 不要求安装后为 sandbox 重启系统；
- 不把 POC 接入 Desktop / Runner production path；
- 已通过 `65a97891` 普通 revert 保留历史记录并把 POC 从产品主线移除，没有 reset / 改写历史；
- **Batch F — Destructive Guard & Bounded Recursive Delete 已完成本地 focused Gate。**

CodexLink-managed Runner 现在通过受控 `project activate --allow-raw-shell` 从初始 fail-closed enrollment 切到 normal execution；该切换继续走既有 config check / generation CAS / hot reload。WebCodex 通用 login/connect 默认不因此放宽。

### 首版 UI / UX 已正式冻结

- 首版恢复并沿用已验收 RC5 产品壳，不使用当前 rebuild Dashboard 作为最终视觉方向；
- 权威参考是 `D:\Test\CodexLink\Current-RC`、真实安装形态、历史 Archive/Rejected/Rollback、已保存截图和真实行为；
- RC5 对应 product source commit `0f63d08e...` 当前不在现有 Git object 中，`CodexLink-stable-v1` 也没有可恢复源码；如果以后找到真实源码优先复用，否则按成品行为恢复；
- 用户主链固定为：激活 → 选项目 → 设置里保存 Tunnel ID + Restricted Runtime Key → 去 ChatGPT 创建/添加 App；Server / Runner / 端口 / tunnel-client / 普通代理配置默认自动处理和隐藏；
- 详细视觉与交互契约见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。

### 商业激活与销售闭环已正式冻结

- `MYWS-...` 销售激活码不是单纯下载密码，而是 CodexLink 的购买 entitlement + 首次设备激活凭证；
- Deployment Center 先用激活码验证购买资格并展示下载 / 教程，网站验证本身不等于设备绑定；
- 安装后首次启动继续使用**同一枚激活码**完成设备激活；
- 未获得有效本机 Signed License 前，不启动 Server / Runner / Tunnel 等可工作 runtime；
- 已激活设备通过本地 Signed License 验证，正常启动不依赖网站持续在线；
- 商业合同与网站 Gate 见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

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

Batch F 的过渡 Safe Mode Gate 已完成：CodexLink-managed Runner 只在 destructive admission、typed delete 和 focused Windows regressions 通过后，通过受控 project activation 恢复 normal execution；通用 WebCodex enrollment 仍可保持 `allow_raw_shell=false`。

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
| P0 | Product identity / state / migration / device activation / Tunnel | **已完成** | Batch A-D focused tests 已通过；设备 Activation Gate 已恢复 |
| P0 | Lifecycle / installer | **本地完成** | `7ebab38d`；待 GitHub TLS 恢复后 push，真实 upgrade 仍留给新 RC |
| P0 | Batch F：Destructive Guard & Bounded Delete | **本地完成** | typed 文件合同 + bounded recursive delete + destructive admission + focused Windows regressions 已通过 |
| P0 | 恢复正常 model execution | **本地完成** | CodexLink-managed Runner 经 checked project activation 开放 shell/process/script；通用 WebCodex 默认不放宽 |
| P0 | Batch G：RC5 Product Shell + 极简配置 UX | **本地完成** | 四项主导航、Activation Gate、RC5 首页/项目/连接/设置心智与极简 Tunnel 配置已恢复；真机视觉对照留给新 RC |
| P0 | Batch H：商业激活 / Deployment Center MVP | 待做 | 同一 MYWS 码完成“验证购买并下载 → Desktop 设备激活”；网站验证与设备绑定语义清楚 |
| P0 | Batch I：Windows 新 RC 商业全链验收 | 待做 | verify purchase → download → install → activate → project → Tunnel → ChatGPT App → files/execution/guard → restart/upgrade/uninstall |
| P1 | 网站内容与小白图文教程收口 | 待做 | 使用真实新 RC 截图、当前 OpenAI 路线、版本历史、支持与安全边界说明 |
| P1 | Batch J：正式商业发布准备 | 待做 | 签名 installer、SHA、归档、第三方许可、最终 clean-machine 验收 |
| P2 | UI V2 | **仅在未来明确立项时** | 首版不自动 redesign 已敲定的 RC 产品壳 |

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

客户端设备激活已经恢复；首版商业合同进一步固定为“同一销售激活码，两阶段使用”：

```text
MYWS 销售激活码
├─ Deployment Center：验证购买资格 / 查看下载
└─ CodexLink Desktop：首次设备激活 / 获得 Signed License
```

约束：

- `MYWS-...` 是产品 entitlement，不只是下载密码；
- 网站验证购买资格不应伪装成设备已激活，也不应因为查看下载页就自动完成设备绑定；
- 首次启动使用同一枚销售激活码进入 Activation Gate；
- 未激活时 Server / Runner / Tunnel runtime 不偷跑；
- 激活码提交后不长期保存到普通 Desktop state；
- 已激活设备通过 Signed License 本地验证，不要求每次启动都在线；
- 页面只显示客户需要知道的信息，不暴露 Ed25519、DPAPI、签名 payload、内部 endpoint 等实现；
- 私有签名 key 永不进入客户端；
- 详细商业合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

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

产品核心恢复 Gate 已基本满足；Batch E 只剩 remote backup 与真实 RC upgrade 证据。首版不做新的视觉 redesign，但**已验收 RC5 Product Shell 的恢复属于首发 P0**，在 Batch F 后、新 RC 前完成。

---

## 7. Stage 3 — Batch F：Destructive Guard & Bounded Recursive Delete

详细规范：[`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。

**状态：本地实现与 focused Gate 已完成。** 新 RC 仍需在真实 Windows 上复验 normal execution、Guard 拒绝与大型目录清理。

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

本地 focused Gate 已通过：normal execution 的受控恢复路径、Destructive Guard、typed `delete_directory`、tool/registry contracts 和 model guidance 均有回归证据。明显 recursive delete、root/wildcard destructive target、destructive Git、mirror/purge、disk destructive primitive 会在自由执行入口 dispatch 前拒绝；typed recursive delete 使用单遍 walker 且不 follow reparse point。

真实 Windows RC 仍必须复验大型 generated tree、junction/reparse、正常 Cargo/npm/Python/Node/PowerShell 工作流与“拒绝后无 child spawn”的端到端行为；这属于 Batch I 真机 Gate，不重新阻塞 Batch G 产品壳开发。

---

## 7.5 Stage 3.5 — 恢复 RC5 Product Shell 与极简配置链

详细规范：[`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。

### 首版恢复目标

- 首页：`一切就绪`、当前项目大卡片、连接链路、接下来做什么、系统状态；
- 项目：`此电脑上的项目` 卡片式管理；
- 连接：`ChatGPT 连接` 步骤链、当前状态、Secure Tunnel 卡片；
- 设置：语言、登录启动、Tunnel ID、Restricted Runtime Key、必要时才出现的高级网络设置；
- 保留 RC 成品的克制动效、卡片、状态点和页面节奏；
- 不把当前 rebuild Dashboard 的底层 runtime 信息直接推给普通用户。

### 极简配置 Gate

普通客户首次使用只需要：

```text
激活
→ 选择项目
→ 设置中保存 Tunnel ID + Restricted Runtime Key
→ CodexLink 自动准备 Engine / Runner / Secure Link
→ 在 ChatGPT 创建/添加 App
→ 使用
```

Server URL、Runner enrollment、loopback 端口、tunnel-client path、health URL、正常情况下的代理选择都不得成为首用步骤。

### 历史证据

`D:\Test` 下的 Current-RC / Archive / Rejected / Rollback / CodexLink-Test 继续作为只读恢复证据。若后续找到 RC5 原始 UI 源码，优先直接复用；当前未找到时，以真实 RC 行为和截图恢复。

---

## 7.6 Stage 3.6 — 商业激活与 Deployment Center MVP

详细合同：[`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

新 RC 外测前，先把真实客户会走的购买 / 下载 / 激活入口收口到可验收状态：

```text
销售激活码
→ deploy.syzdq.xyz 验证购买资格
→ 查看并下载当前推荐 CodexLink
→ 安装
→ Desktop 使用同一枚激活码完成设备激活
```

### MVP Gate

- 有效 `MYWS-...` 码可以看到正确推荐下载、版本和 SHA；
- 无效码不能进入受保护的购买下载入口；
- 页面明确说明该激活码还要用于 CodexLink 首次设备激活；
- 网站验证购买资格不等于设备绑定；
- Desktop 未激活时不启动可工作 runtime；
- 同一枚有效销售码能完成 fresh-machine Desktop 激活；
- 已激活设备正常启动不依赖 Deployment Center 持续在线；
- 网站 / Desktop / 教程使用一致的“销售激活码 / 激活 CodexLink”产品词汇。

这个阶段只要求商业主链真实可用，不先引入账号系统、自动更新、CDN 或复杂下载 token 体系。

---

## 8. Stage 4 — Windows 真实机端到端验收

每个候选必须在真实 Windows 上验证，不只跑单元测试。

### 最小全链

1. 使用一枚有效测试销售激活码进入 Deployment Center；
2. 验证购买资格并取得当前推荐 installer / SHA；
3. clean/fresh install（另测 custom install path）；
4. 首次启动使用**同一枚销售激活码**完成 Desktop activation；
5. 验证未激活状态下 Server / Runner / Tunnel 不偷跑；
6. 选择真实项目；
7. 在设置中只填写并保存 Tunnel ID + Restricted Runtime Key；
8. CodexLink 自动把 Server / Runner / Secure Link 准备到 ready，不要求用户配置底层组件；
9. ChatGPT Developer Mode / 创建或添加 App；
10. App 连接后首页与连接页进入真实 ready/verified 状态；
11. typed read / search；
12. typed create + guarded edit + Git diff；
13. single-file delete；
14. bounded recursive `delete_directory`（使用 disposable test tree）；
15. normal shell / build / test；
16. destructive raw shell 被 Guard 拒绝且没有 child spawn；
17. project root / parent / reparse destructive regression；
18. 重启 CodexLink 后项目、激活和 Tunnel 凭据自动恢复；
19. Windows 登录 autostart（启用时）；
20. upgrade 覆盖安装并复用正确状态；
21. uninstall 后验证持久用户数据策略；
22. reinstall / downgrade（在兼容范围内）。

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
01 输入销售激活码，验证购买并查看下载
→ 02 下载并安装 CodexLink
→ 03 首次启动，用同一枚激活码激活 CodexLink
→ 04 添加本地项目
→ 05 创建 OpenAI Tunnel
→ 06 创建 Restricted Runtime Key
→ 07 在 CodexLink 保存两项 Tunnel 凭据
→ 08 开启 ChatGPT Developer Mode
→ 09 创建/添加 CodexLink App
→ 10 Scan Tools / 首次使用
```

网站必须明确告诉用户：**销售激活码不要在下载后丢弃，它还用于 CodexLink 首次设备激活。**

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

## 11. Stage 7 — 首发以后可选的 UI V2

首版已经正式选定 RC5 Product Shell 作为产品心智，不再默认安排一次“大改版”。

首发以后先根据真实客户反馈微调：

- 哪些状态仍然难懂；
- 哪个首次配置步骤还可以消失；
- 项目/连接页是否需要更少文字；
- 动效、键盘、高 DPI 和可访问性是否需要改进。

只有用户以后明确决定重做信息架构时，才单独立项 UI V2。默认继续保持 `首页 / 项目 / 连接 / 设置` 与 RC5 的核心交互模型。

---

## 12. 当前下一步顺序

从 Batch F 收口后，严格按以下顺序推进：

1. **已完成：商业 / UI / 安全决策入档。** 安全边界、RC5 Product Shell、极简配置链和“同一 MYWS 码 = 购买 entitlement + Desktop 激活”均为首版权威合同。
2. **已完成：移除 Sandbox POC 产品主线。** `65a97891` 普通 revert，历史保留，没有 reset / 重写。
3. **已完成：Batch F + normal execution 本地 focused Gate。** Bounded Recursive Delete、Destructive Guard、model guidance 与 checked activation opt-in 已通过 focused regressions。
4. **远端备份待网络恢复。** GitHub TLS 恢复后把尚未推送的本地 checkpoint 普通 push 到 `origin/codexlink-rebuild`，不使用 TLS 绕过。
5. **已完成：Batch G — RC5 Product Shell 本地实现。** 主导航固定为首页/项目/连接/设置；首页、项目、连接、设置恢复 RC5 产品心智；首次产品配置收敛为“激活 → 项目 → 两项 Tunnel 凭据 → ChatGPT App”；Activity 只保留为诊断入口；真实 RC5 逐页视觉/高 DPI/动效对照留给新 RC 真机验收。
6. **下一步：Batch H — Deployment Center 商业 MVP。** 销售激活码验证购买、下载入口、版本/SHA、同码设备激活说明和最小 Quick Start 真正可用。
7. **Batch I — 新 RC 商业全链。** verify purchase → download → install → same-code activation → project → Tunnel → ChatGPT App → files/execution/guard → restart/upgrade/uninstall。
8. **外部 clean-machine 测试。** 至少一台从未安装 CodexLink 的朋友/测试机按网站教程从下载资格开始全程复跑，不使用内部捷径。
9. **修正 RC 反馈并重新候选。** 发现问题就修，不覆盖 accepted/rejected 历史 artifact。
10. **用最终 RC 截图完成网站内容。** 图文教程、FAQ、版本历史、支持、安全边界和真实 OpenAI 路线收口。
11. **Batch J — 正式发布准备。** Windows 签名、SHA、归档、第三方许可、最终 clean-machine acceptance。
12. **Stable 首版发布。** UI V2 只有以后明确立项才推进。

任何阶段发现会扩大 destructive scope、权限、credential exposure、历史覆盖或不可逆迁移时，立即 stop-the-line，先解决边界再继续。
