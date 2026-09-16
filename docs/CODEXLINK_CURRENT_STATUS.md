# CodexLink Current Status

> 状态快照：**2026-09-16，源码安全审计五项整改已完成代码 checkpoint；Stage E 自动化回归、文档与 secret scan 已收口，最终交付要求 `5ff6d8fd → final HEAD` range review 与 origin SHA confirmation。**
>
> 用途：记录当前 `codexlink-rebuild` 工作树的真实状态、已验证能力、尚未完成 Gate 和下一步顺序。本文优先描述“现在是什么”，长期产品合同仍分别以 Security Boundary、First-Release UI、Commercial Flow 等专题文档为准。

## 1. Git / source snapshot

- 仓库：`D:\GitHub\CodexLink`
- 分支：`codexlink-rebuild`
- 安全恢复锚点：`5ff6d8fda07e5172f272e07b8b4b9bdd74ad0bb7` — `CodexLink: checkpoint pre-scale-test hardening`；不得改写历史。
- 本轮源码整改已形成三个本地代码 checkpoint：`d98eeb8b`（arbitrary execution boundary）、`fca59435`（Windows enrollment credentials）、`ede713a0`（Remote profiles / zero-root / topology readiness）。
- 本文只记录源码/Gate事实，不假定网络 push 成功；最终交付必须额外核对 `origin/codexlink-rebuild` 的实际 SHA 与 final HEAD 完全一致。
- 之前真实 bundled CodexLink Engine identity 为 `0.4.1 / ae858f5360ff / dirty=false`；本轮源码已经发生整改，`ae858f...` 只作为整改前 dogfood 证据，不能作为下一次 Desktop / Engine 重建后的最终 source identity。
- 历史 BrandFix installer `target\CodexLink_0.4.1_RC5-BrandFix_x64-setup.exe` / SHA256 `B67AB5388095EAB2A256AA2E9EF94231A94E87FE8ED7EBCB939F0D9159C8FF71` 继续作为整改前包证据，不代表本轮整改后的新安装包。

历史恢复基线 `26c44e6436f254183d74db6865d04e2ec1d85930` 仍作为最初 0.4.1 上游证据；当前实际 dogfood runtime 已经是 CodexLink fork 上的 `ae858f...` 构建，不能再把两者写成同一个 source identity。

## 2. 当前产品架构

CodexLink 首版当前固定为三层：

```text
CodexLink Product Shell
  ├─ activation / commercial entitlement
  ├─ projects / connection / settings UX
  ├─ installer / tray / lifecycle
  └─ customer-safe diagnostics
            ↓
CodexLink Compatibility + Safety Layer
  ├─ internal runtime anchor
  ├─ user authorized roots
  ├─ desired-state recovery
  ├─ Tunnel routing / secret storage
  └─ Destructive Guard / bounded delete
            ↓
Bundled WebCodex 0.4.1 Runtime
  ├─ CLI
  ├─ Server
  └─ Runner
```

### 2.1 Internal runtime anchor 与用户授权路径已经分离

当前本机模式不再把“Runtime identity”误当成“用户当前工作区”：

- CodexLink 使用内部 `runtime-anchor` 维持 Engine / Runner 生命周期；
- `projects[]` 只保存用户明确授权给 ChatGPT App 的盘符或文件夹；
- internal anchor 不应出现在普通“项目”页面；
- 用户可以同时保留多个授权 root；
- 添加/移除项目只改变 authority，不再切换 Runtime identity；
- 0 个用户项目时，Server/Runner Runtime 连接可以是健康的；但 `ready_for_chatgpt=false`，因为还没有任何 customer-authorized project 可以交给 ChatGPT 使用；UI 应提示添加授权路径，而不是把内部 anchor 当项目。
- Local 与 Remote 都允许真正的 0 authorized roots；CodexLink-managed enrollment 不再用 HOME、盘符根或 internal runtime anchor 偷偷填充 customer authority。
- internal runtime anchor 只用于 Local Runner 恢复，不再投影到普通项目 UI；Remote enrollment 直接使用 Runner identity，因此也不需要用“假项目”维持连接。

### 2.2 本机与 Remote Server 是两种不同拓扑

- 默认本机模式：CodexLink 管理本机 Server + Runner + OpenAI Secure Tunnel。
- Existing Server 模式：远端保留 Server，CodexLink Desktop 只 enrollment/监管本机 Runner。
- Remote 模式的长期 enrollment 已与 active topology 解耦：Desktop 会按 normalized Server URL 从 `connections` profile 中发现并复用 Runner identity，Local 与多个 Remote enrollment 可以并存。
- 切到 Local、重启 CodexLink 或 Windows 后，再切回同一个 Remote Server 不应因为 active config 已变成 `127.0.0.1` 就要求重新 pairing。
- Remote setup 采用事务语义：先验证 saved profile / pairing、Server revision、Runner 和可选 project，再提交 active topology；失败保持原 Local/Remote topology，不留下“半成功 Remote UI”。
- transient network/offline 与 credential invalid 已分开：断网保留 enrollment 并重试；401/403 才进入 credential invalid / re-enroll；client identity mismatch 与 revision mismatch 保持独立错误。
- Remote Server readiness 来自真实 probe；HTTPS Remote 不再依赖本机 OpenAI Tunnel readiness，HTTP Remote 会显示 degraded，而 URL 已保存本身不代表绿色 online。

因此旧 WebCodexLauncher **现在还不能删除**：它仍是本轮维修通道；等重新 build CodexLink Desktop + CodexLink Engine，并在同源 Remote Server 上完成真实 restart/reuse smoke 后再退休。

## 3. Product Shell / UI 状态

Smoke 3 期间已经从“恢复 RC5 大概样子”推进到产品语义和交互基本收口：

- 主导航固定为 `首页 / 项目 / 连接 / 设置`；顶部“当前工作区”已从普通页面删除；
- 首页主卡只显示第一个用户授权路径；0 项目时显示“等待配置授权路径”；
- 首页连接链路按 topology 投影：Local 为 `本机 → Engine → Secure Link → ChatGPT`，Remote 使用真实 `远程 CodexLink Server` 节点；0 项目时 Runtime 可健康，但 ChatGPT 节点保持 pending，直到用户授权至少一个项目；
- 项目页是真正的多授权 root 管理，不是单一 active workspace；支持显式盘符根和普通文件夹；
- 项目卡响应式 3 / 2 / 1 列；移除项目是 authority revoke，不删除磁盘文件；
- `ChatGPT 已访问` 产品语义改为 `ChatGPT 可访问` / 系统状态 `已连接`，不再要求先发生一次真实 tool call 才点亮健康状态；
- “接下来做什么”、系统状态、渐变主卡、hover/page transition 已按 RC5 心智收口；
- 设置页只保留用户需要的 Tunnel 凭据、语言、登录启动和高级网络；内部诊断信息已从普通 UI 移除；
- Tray 当前产品目标只保留“打开 CodexLink / 退出 CodexLink”；语言跟随 Desktop locale；
- 6 个用户语言包已经增加大小写不敏感品牌守卫，customer-facing i18n 不允许重新出现 `WebCodex` 产品品牌。

Windows Explorer 图标资源已经证明最终 EXE 内嵌图标与当前蓝色 CodexLink 32px 资源一致；Explorer 历史缓存曾导致显示旧图标。缓存处理造成过一次 Explorer Shell 被错误回收的事故，后续**禁止再通过远程脚本杀 Explorer / Shell 来刷新图标**。只允许 installer hook / `SHChangeNotify` 等非破坏路径。最终 Explorer 展示仍等待一次干净安装后的用户确认。

## 4. Activation / commercial state

### 4.1 设备激活

当前 Desktop Activation Gate 已具备：

- `MYWS-...` 销售码首次设备激活；
- Signed License + Ed25519 public verification；
- CurrentUser DPAPI 设备身份；
- 未激活时 runtime fail closed；
- 激活错误按 `error.code` 进入 i18n presentation，不再把后端英文 `next_action` 直接显示给中文用户；
- customer-facing i18n 已统一 CodexLink 品牌。

设备激活材料现在目标位置为：

```text
%LOCALAPPDATA%\CodexLinkSecretKey\
  device_identity.dpapi
  license.json
```

它与 `%LOCALAPPDATA%\dev.codexlink.desktop` 的普通 app/runtime state 分离。卸载器即使删除普通 app data，也不应等价于解绑设备。当前代码支持从：

1. `%LOCALAPPDATA%\CodexLink.DeviceActivation.v1\`；
2. 更旧的 `%LOCALAPPDATA%\dev.codexlink.desktop\license\`

向 `CodexLinkSecretKey` 做兼容迁移，并保留 fail-closed / 不覆盖不同身份的规则。

**仍需真机再次明确确认：**安装 BrandFix → 已激活 → 卸载并删除普通应用数据 → 重装后仍直接识别同一设备。自动化回归已经通过，但该真实卸载路径还应在最终 RC Gate 再跑一次。

### 4.2 协议内部品牌

`license.json` 里的签名字段 `product: "webcodex-deployment"` 当前**故意保留**。它属于服务端签名 payload / 协议兼容字段，不能只在客户端改名，否则现有签名立即失效。未来若要改成 CodexLink，需要授权服务签发端与客户端验证端一起做协议版本迁移。

同理，`webcodex.exe`、`webcodex-server.exe`、`webcodex-runner.exe`、`WebCodexAdapter` 等技术名属于 Engine 内部兼容面，不等同于 customer-facing 品牌泄漏。

### 4.3 License Operator

Batch H.0 已提交并推远端：`4b4043ca`。

Windows 管理员 GUI 已完成真实一键签发体验：通过管理员 SSH key 固定调用 `/usr/local/bin/myws-license-admin issue --json`，销售码不落盘，签名私钥不离开服务器，非幂等 `issue` 永不自动重试。

Deployment Center / 商业下载 Gate 仍未作为 CodexLink repo 内的完整 MVP 收口，继续属于后续商业发布工作。

## 5. Runtime lifecycle / recovery

当前工作树已实现：

- 激活成功后重新触发 runtime bootstrap，而不是必须重启应用；
- 已保存 Tunnel ID / Restricted Runtime Key 且用户未明确停止时，启动会自动恢复 Runtime + Secure Link；
- 周期状态观察发现受管 Server / Runner / Tunnel 意外掉线时，可以进入自动恢复链；
- 首页黄色状态提供统一“恢复连接”兜底，而不是要求用户理解“先启动 Runner、再重连 Tunnel”；
- 用户明确点击停止 Runtime / Tunnel 时，desired-state 会阻止自动偷偷拉起；
- 自动恢复后应清除旧错误卡，避免“链路全绿但顶部仍报错”。

前端已经为“运行一段时间后 Tunnel 再掉线”的场景增加回归测试；真实用户长时间运行中的断线自愈仍继续观察，发现问题再修。

## 6. Project authority

当前项目模型已经从历史的单 `project` 产品心智迁移为多授权路径：

- `projects[]` 是用户可见 authority 集合；
- 旧 `project` 仅保留必要的向后兼容/内部 runtime state；
- `project activate` 可以只增加授权，不要求切换 Runtime identity；
- 新增 `project remove` 用于精确撤销 authority；
- 若一个待移除子目录仍被更宽的父授权 root 覆盖，必须拒绝“假撤权”，提示先处理父授权；
- 删除最后一个授权路径不能留下一个隐藏、仍可被 ChatGPT 使用的用户权限 root；
- internal runtime anchor 不属于用户项目列表。

## 7. Security boundary

本轮把安全能力明确拆成两类：

- **Typed Operations**：typed read/write/edit/rename/delete、bounded recursive delete、typed Git、exact authorized roots、expected-SHA / CAS、canonical/reparse checks；这是 CodexLink Engine 可以承诺的强权限边界。
- **Arbitrary Execution**：raw shell / process / script / PowerShell / Python / Node 及其内部 subprocess；这是“允许 AI 以当前 Runner Windows 用户权限执行项目命令”的高级 capability，不是 filesystem/OS sandbox。`cwd/allowed_roots` 只约束 CodexLink 管理的路径与启动工作目录，不能收回 child process 的 OS 权限。
- Destructive Guard 保留为 pre-spawn 的高置信误操作 mitigation，不再宣称能完整分析 arbitrary code；本轮补齐 PowerShell `-EncodedCommand/-enc/-ec/-e` 这种直接 opaque 入口，但不走无限解释器 denylist。
- typed create/edit/rename/delete、bounded `delete_directory`、root/parent/wildcard/protected anchor/reparse fail-closed 等既有强边界继续保持。
- Windows 长期 bearer credential（user token、Runner token-bearing `runner.toml`）现在与已有 Server secret 共用同一 protected secret-file primitive：当前用户 + SYSTEM FullAccess、禁用继承，ACL/reparse hardening 失败时 fail closed，overwrite 后不得退化。

**下一项最高优先级动态安全 Gate**仍是 `tunnel-test` disposable-root 专项：验证 normal execution、direct destructive deny、no-child-spawn，以及 junction/reparse TOCTOU 等本轮明确未重构的动态风险。不得拿真实源码仓库或真实用户文件做 destructive 负面测试。

## 8. 当前两套本地开发/连接链路

目前机器上同时存在两套链路：

### 旧链路（暂时保留）

```text
WebCodexLauncher 0.7.2
→ WebCodex 0.3.9
→ 旧公网 Server / Runner enrollment
```

它仍承担当前网页会话中的一部分开发访问能力，因此在 Remote Server 迁移完成前不要删除。

### 新链路（CodexLink dogfood）

```text
CodexLink Desktop
→ bundled WebCodex 0.4.1 ae858f...
→ CodexLink-managed Server/Runner 或 Existing Server 模式
→ tunnel-test / ChatGPT
```

`tunnel-test` 已经能看到/操作 CodexLink 0.4.1 Runner，是后续安全验收和最终替换 Launcher 的目标链路。

## 9. 当前验证证据

Stage E 已实际运行的自动化证据：

- Frontend TypeScript typecheck：通过；
- Frontend 全部 Vitest：`60/60`，并通过 form-control CSS contract；
- Frontend production build：通过；
- Rust Desktop `--all-targets`：`121 passed / 0 failed / 5 ignored`；5 个 ignored 均为既有显式 real-process/dogfood lane；
- `webcodex-runner-config`：`31/31`；
- `webcodex-cli --lib`：`318/318`，其中包含 Windows credential DACL overwrite regression；
- P1 Destructive Guard focused：`7/7`；`webcodex-core` structured process encoded/shell-command-mode regression：`1/1`；tool contracts：`95/95`；
- Remote profile URL normalization / ambiguous profile / transient-vs-revoked classification、0-root、rollback、Remote readiness focused tests：通过；
- `cargo fmt --all -- --check` 与 `git diff --check`：通过；
- 从 `5ff6d8fd` 到当前 worktree 的 bounded secret scan 对 pairing/user/agent token、private-key header、OpenAI secret-key / literal assignment 均为 `0` 命中。

**Workspace full-suite caveat：** `cargo test --workspace --lib` 在 Windows 的根 `webcodex` 巨型 test binary 中出现 suite-order `STATUS_STACK_OVERFLOW`。首次触发在 `runtime_console_http::...window_activity_lookup...`；该测试单独 `1/1` 通过且对应源码自 `5ff6d8fd` 起无 diff。串行 `--test-threads=1` 仍复现；跳过该条后又在更晚的既有 `tool_runtime::tests::jobs::list_jobs...` 发生同类 stack overflow，该条单独也 `1/1` 通过且对应源码无 diff。因此不能宣称“workspace combined suite 全绿”，也不在本轮五项整改范围外修改这些测试。其余本轮相关 focused/产品 Gate 如上均为绿色。

这些自动化证据仍不替代重新 build 后的同源 Remote 真机 smoke。

## 10. 仍未完成 / 不应误报完成

1. **Destructive Guard / junction-reparse 的 `tunnel-test` 动态安全专项**尚未完成；
2. 本轮整改后还没有重新 build 新的 CodexLink Desktop + CodexLink Engine artifact；
3. **Existing Server 模式还没有用整改后同源新 build 完成一次真实 Local → Remote → Local → same Remote / restart / outage / revoke smoke**；
4. **WebCodexLauncher 还不能退休**；
5. BrandFix 的**设备激活跨卸载真实复验**仍应再跑一次；
6. Explorer 图标显示在缓存清理后的**最终用户确认**仍待收口；
7. Runner / Tunnel 长时间运行中的**真实意外断线自愈**继续观察；
8. Deployment Center 商业 MVP、code signing、third-party notices、外部 clean-machine acceptance 仍未完成。
9. 根 `webcodex` Windows 巨型 lib test binary 存在既有 suite-order stack-overflow 稳定性问题：触发用例单独运行通过、相关源码不在本轮 diff；本轮记录为独立测试基础设施/隔离风险，不扩大为第六项整改。

## 11. 下一阶段严格顺序

建议继续按以下顺序推进，避免再把风险面混在一起：

1. **Stage E 收口本轮源码整改**：全量回归、文档、secret scan、committed-range review、普通 push；
2. **tunnel-test 动态安全专项**：正常开发命令 + destructive deny + no-child-spawn + junction/reparse race；
3. 重新 build CodexLink Desktop + CodexLink Engine，并记录新的 exact source identity；
4. 将同源 Engine Server 部署到 Remote 测试端，跑 Local → Remote → Local → same Remote、Desktop/Windows restart、transient outage、credential revoke、0-root 与 project add/remove smoke；
5. 用 `tunnel-test` / ChatGPT 通过远端 Server 做 read/write/normal execution/Guard smoke；
6. 上述链路稳定后，**再退休 WebCodexLauncher 0.7.2 / WebCodex 0.3.9**；
7. 宣布进入**规模测试阶段**：多项目、不同代理、断线恢复、upgrade/uninstall/reinstall、外部 clean-machine；
8. 最后收口 Deployment Center、code signing、third-party notices 与 Stable release。

## 12. 操作红线

- 不删除/覆盖 `D:\Test` 历史 RC 证据；
- 不 force-push / reset published history；
- 不在安全专项完成前替换并删除唯一可工作的旧远端链路；
- 不为了刷新图标远程 kill Explorer / Shell；
- 不把 internal runtime anchor 重新当成 user authorized root；
- 不把 signed-license 协议字段单边改名；
- destructive 测试只在一次性 disposable test root 内进行；
- 任何 credential exposure、权限扩大、不可逆 migration、历史覆盖，立即 stop-the-line。
