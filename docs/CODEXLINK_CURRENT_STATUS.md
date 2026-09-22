# CodexLink Current Status

> 状态快照：**2026-09-22，v1 Local-only 功能、安全边界、clean-machine 主链与三轮源码审计均已收敛；最终二进制审计曾真实复现“bundled Core 顶层单分支 patch 可绕过 activation 并进入 `server init`”的商业授权弱点，现已完成 targeted commercial hardening。优化 dogfood 二进制已证明 startup admission 与 CLI / Server / Runner 第二阶段 admission 分离，合法激活可正常穿过两层 gate、空激活三份 Core 均 fail closed，旧 product-mode 成功 probe / 可读逆向锚点已移除。下一步只剩从最终冻结 HEAD 重建 hardened RC/installer，并对受影响生命周期做窄回归后封版。**
>
> 用途：记录当前 `codexlink-rebuild` 工作树的真实状态、已验证能力、尚未完成 Gate 和下一步顺序。本文优先描述“现在是什么”，长期产品合同仍分别以 Security Boundary、First-Release UI、Commercial Flow 等专题文档为准。

## 1. Git / source snapshot

- 仓库：`D:\GitHub\CodexLink`
- 分支：`codexlink-rebuild`
- 安全恢复锚点：`5ff6d8fda07e5172f272e07b8b4b9bdd74ad0bb7` — `CodexLink: checkpoint pre-scale-test hardening`；不得改写历史。
- 五项整改与 D.1 lifecycle 收口已经形成 `d98eeb8b` → `fca59435` → `ede713a0` → `112d08cc` → `31ef8186`；其中 `31ef8186` 已推送到 `origin/codexlink-rebuild`，并作为当前远端安全基线。
- 五项整改的独立审计范围仍固定为 `5ff6d8fd → 48f9af16`；D.1 saved-Remote Runner replacement 与 failure rollback 另行定点复审并通过，不重新打开整改范围。
- 当前首发产品源码已经继续推进到 commercial hardening checkpoint `b48b3cf1bcc4d41e519ad8845b94391c8c556a50`。其中 `8f8b71d9` 把 bundled Core 从单一 boolean admission 改为 verified activation fingerprint + process/binary-bound runtime seal + CLI/Server/Runner 第二阶段 admission；`b48b3cf1` 进一步删除旧 product-mode success probe 与新加入的可读 stage/fingerprint domain 文本，避免主动提供逆向锚点。这两次 hardening 都只在本地 `codexlink-rebuild`，未 push。
- hardening 前最后一个完整 Final RC installer 来自 `7ff41acd7a9eb08089609dd42539f3fcc4961914`：`CodexLink_0.4.1_7ff41acd7a9e_finalrc_x64-setup.exe`，SHA256 `43296E305FF2E28D049596A529BB8C8DA0A674DC78BFBCFB6897683BE6447BB1`。该包已完成此前 clean-machine 11/11 与运行时安全抽检，但它**早于 commercial hardening**，现在只保留为功能/生命周期证据，不再作为最终销售候选。
- `b48b3cf1` 已生成 optimized dogfood Core 作为 hardening binary evidence：Core SHA256 `C8A2C5EE070E7BCDCC340C67A4EE71154F5C8651C8A06E74C5D5A1D903D0272F`；Server `7FFC0C692498971D909D82B9ABCC375359CF796A80846F9274F10289B3ED392D`；Runner `89F96D0080874E400378E86DC0102FBE8B0D918D342FC9653134FB7B7A3099FF`；三者 `dirty=false`、`built_at=1790046019`。对应 staging 为 `target\codexlink-hardening-bundle-b48b3cf1bcc4`，metadata `product_mode=verified-fail-closed-v2`。
- 历史 `d1c23c8a` hands-on installer、`ae858f...` runtime 与 BrandFix installer 继续只作为历史证据，不再代表当前测试线。

历史恢复基线 `26c44e6436f254183d74db6865d04e2ec1d85930` 仍作为最初 0.4.1 上游证据；当前 hardened product checkpoint 是 `b48b3cf1`，而 `7ff41acd` Final RC、`d1c23c8a` hands-on 与更早 `ae858f...` 必须分别作为不同阶段证据记录，不能混写成同一个 source identity。

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
CodexLink Core (bundled Engine 0.4.1)
  ├─ Core CLI
  ├─ Core Server
  └─ Core Runner
```

### 2.1 Internal runtime anchor 与用户授权路径已经分离

当前本机模式不再把“Runtime identity”误当成“用户当前工作区”：

- CodexLink 使用内部 `runtime-anchor` 维持 Engine / Runner 生命周期；
- `projects[]` 只保存用户明确授权给 ChatGPT App 的盘符或文件夹；
- internal anchor 不应出现在普通“项目”页面；
- 用户可以同时保留多个授权 root；
- 添加/移除项目只改变 authority，不再切换 Runtime identity；
- 0 个用户项目时，Server/Runner/Secure Link 可以是健康的，ChatGPT transport 也可以显示绿色“可访问”；但 aggregate `ready_for_chatgpt=false`，因为还没有任何 customer-authorized project 可供工具使用。UI 应继续提示“等待配置授权路径”，而不是把内部 anchor 当项目或把 transport 可达误写成项目已就绪。
- Local 与 Remote 都允许真正的 0 authorized roots；CodexLink-managed enrollment 不再用 HOME、盘符根或 internal runtime anchor 偷偷填充 customer authority。
- internal runtime anchor 只用于 Local Runner 恢复，不再投影到普通项目 UI；Remote enrollment 直接使用 Runner identity，因此也不需要用“假项目”维持连接。

### 2.2 v1 产品表面冻结为单一 Local 拓扑

v1 正式用户只走：`本机 CodexLink Core → OpenAI Secure Tunnel → ChatGPT`。普通界面不再提供 topology chooser，也不再显示 `连接现有 Server`、`快速共享项目` 或 `其他配置方式`。

Existing Server / Remote enrollment 与 Quick Share 的底层实现**不删除**：前者已经完成 saved profile、transactional topology、0-root、真实 readiness 与 D.1 Runner replacement/rollback 源码整改，后者也继续留在代码和测试中；但两者从 v1 customer surface、首发教程与 release gate 移到 hidden experimental / Post-v1。后续只有真实用户需求证明值得产品化时，才重新开放高级入口并单独做同源 Server / Remote lifecycle 验收。

旧 WebCodexLauncher 继续留在开发机作为维修/历史回滚通道，但**不再是 v1 产品依赖，也不再以 Remote Server 迁移完成作为 v1 发布 Gate**。首发只需证明 CodexLink 自己的 Local + Tunnel 主链在 Launcher 关闭时正常工作。

## 3. Product Shell / UI 状态

Smoke 3 期间已经从“恢复 RC5 大概样子”推进到产品语义和交互基本收口：

- 主导航固定为 `首页 / 项目 / 连接 / 设置`；顶部“当前工作区”已从普通页面删除；
- 首页主卡只显示第一个用户授权路径；0 项目时显示“等待配置授权路径”；
- v1 首页连接链固定为 `本机 → Engine → Secure Link → ChatGPT`；0 项目时前 3 层与 ChatGPT transport 可真实显示绿色“可访问”，但整体仍保持“待完成配置 / 等待配置授权路径”，直到用户授权至少一个项目；
- 项目页是真正的多授权 root 管理，不是单一 active workspace；支持显式盘符根和普通文件夹；
- 项目卡响应式 3 / 2 / 1 列；移除项目是 authority revoke，不删除磁盘文件；
- `ChatGPT 已访问` 产品语义改为 `ChatGPT 可访问` / 系统状态 `已连接`，不再要求先发生一次真实 tool call 才点亮健康状态；
- “接下来做什么”、系统状态、渐变主卡、hover/page transition 已按 RC5 心智收口；
- 设置页只保留用户需要的 Tunnel 凭据、语言、登录启动、ChatGPT 配置教程和必要的高级网络；v1 不暴露 Remote Server / Quick Share 产品入口，内部诊断信息继续从普通 UI 移除；
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

Windows v1 客户安装目录不再暴露 `webcodex-runtime` 或 `webcodex*.exe` 文件名；正式 bundle 映射为：

```text
<CodexLink install>\codexlink-core\
  codexlink-core.exe
  codexlink-core-server.exe
  codexlink-core-runner.exe
  tools\tunnel-client.exe
```

源码内部仍保留 `webcodex*` Cargo package/crate、`WEBCODEX_*` 环境变量、`WebCodexAdapter` 以及构建阶段 `target\dogfood\webcodex*.exe` 等兼容标识。Windows 打包脚本先验证这些同源 Engine bytes 的版本 / commit / `dirty=false`，再按上面的 CodexLink Core 文件名做字节不变的复制映射；这避免在 v1 最后一公里引入 6000+ 处全仓 fork rename。三个 branded Core exe 的内部 `--version` identity 目前仍使用 Engine 的兼容名称，它属于诊断/构建协议，不是 Explorer 文件名。

最终 Windows RC 的品牌 Gate 以**干净安装**为准：新 installer 不应生成任何 customer-visible `webcodex` 文件/目录名。为避免在安装器里新增不必要的递归删除面，不通过 `RMDir /r` 自动清理内部测试时期的旧 `webcodex-runtime`；测试旧包升级时应先正常卸载旧测试包，再验证 clean install。

### 4.3 Bundled runtime commercial hardening

2026-09-22 的最终 binary audit 在旧 `7ff41acd` RC 上真实证明：攻击者无需伪造 Ed25519 或解开 DPAPI，只 patch `codexlink-core.exe` 顶层 activation-result 条件，再配合可伪造的产品父进程，即可在空 activation 环境下让 `server status` 返回正常 JSON，并进一步成功执行 `server init` 生成 bootstrap token。因此旧设计的密码学本身虽然正确，但“所有校验最终汇总成单一 boolean branch”不足以满足首发商业抗低成本 patch 目标。

当前 hardening 不引入 TPM、联网 lease、VMProtect 或新的 DRM 服务，而是只消除该单点：

- startup 与 second-stage 使用两条独立的高层 activation verification 路径；两条路径都会独立完成 DPAPI device identity、device binding 与 Ed25519 signed `license.json` 验证，并在成功后导出同一个 32-byte activation fingerprint；
- operational startup 成功后建立 process-local runtime seal，绑定 activation fingerprint、当前 PID 与 canonical executable path；
- CLI、Server、Runner 在真正进入各自 runtime 前分别执行第二阶段 admission，再验 activation、parent/layout，并核对 startup seal；
- `--version` 保持独立诊断路径；generic/non-bundled Engine 行为保持不变；
- 旧 `--codexlink-product-mode-probe` / `codexlink-bundled-product-v1` 成功 marker 已删除；bundle/smoke 现在通过“故意未知 operational arg 必须 exit 78 + product-denied”证明 fail-closed product mode。

optimized binary 复审进一步确认：三份 binary 的 startup 与 second-stage 不再共享同一个高层 License verifier；每份 binary 均保留两条不同的 activation material verifier 链，并分别进入 startup gate 与 stage gate。runtime-seal / stage proof 继续独立存在；CLI / Server / Runner 各有自己的 stage wrapper。合法 activation + 正确产品布局时三份 binary 均能穿过双 gate，空 `LOCALAPPDATA` 时三份均 exit 78 fail closed；旧 success marker、可读 fingerprint/stage domain 与 `CODEXLINK_BUNDLED_PRODUCT_MODE` 字符串均不在 optimized binary 中。

这项整改的目标是把现实攻击成本从“一处明显 branch patch”提高到必须理解并同时处理多阶段 gate / seal / 多 binary；它**不宣称 Windows 本地 native binary 对专业逆向者不可破解**。如果未来商业规模证明仍需要更强 DRM，再基于真实滥用证据评估 online lease / revocation 或更重保护，不作为 v1 无限扩项。

### 4.4 License Operator

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

**V1 RC 网络提示已收敛（2026-09-17 外部真机反馈）：**当用户所在网络需要代理、但代理未开启而导致 OpenAI control plane 不可达时，Secure Link 仍会按预期 fail closed；`tunnel_proxy_unavailable` 现在单独映射为“无法连接 OpenAI”，直接提示启动用户平时使用的代理，或前往“设置 → OpenAI Tunnel 网络”选择可用代理/直接连接后重试。实现不猜固定代理端口、不自动开启第三方代理，也不把网络不可达误报成 Tunnel 凭据错误。

## 6. Project authority

当前项目模型已经从历史的单 `project` 产品心智迁移为多授权路径：

- `projects[]` 是用户可见 authority 集合，也是 Local Full Runtime 唯一的 customer project authority 来源；
- Local Full Runtime 以 zero-root Runner enrollment 建立内部运行身份；`runtime_anchor` 只保留 Desktop 恢复元数据，不注册 Project、不进入 `allowed_roots`，旧版本留下的 anchor Project identity 会触发一次受控重注册并重放真实 `projects[]`；
- 旧 `project` 仅保留 Remote/向后兼容所需的状态，不再作为 Local internal anchor 的权限载体；
- `project activate` 可以只增加授权，不要求切换 Runtime identity；
- 新增 `project remove` 用于精确撤销 authority；
- 若一个待移除子目录仍被更宽的父授权 root 覆盖，必须拒绝“假撤权”，提示先处理父授权；
- 删除最后一个授权路径不能留下一个隐藏、仍可被 ChatGPT 使用的用户权限 root；
- internal runtime anchor 不属于用户项目列表，也不能出现在 ChatGPT 可读取的 Project inventory。

## 7. Security boundary

本轮把安全能力明确拆成两类：

- **Typed Operations**：typed read/write/edit/rename/delete、bounded recursive delete、typed Git、exact authorized roots、expected-SHA / CAS、canonical/reparse checks；这是 CodexLink Engine 可以承诺的强权限边界。
- **Arbitrary Execution**：raw shell / process / script / PowerShell / Python / Node 及其内部 subprocess；这是“允许 AI 以当前 Runner Windows 用户权限执行项目命令”的高级 capability，不是 filesystem/OS sandbox。`cwd/allowed_roots` 只约束 CodexLink 管理的路径与启动工作目录，不能收回 child process 的 OS 权限。
- Destructive Guard 保留为 pre-spawn 的高置信误操作 mitigation，不再宣称能完整分析 arbitrary code；本轮补齐 PowerShell `-EncodedCommand/-enc/-ec/-e` 这种直接 opaque 入口，但不走无限解释器 denylist。
- typed create/edit/rename/delete、bounded `delete_directory`、root/parent/wildcard/protected anchor/reparse fail-closed 等既有强边界继续保持。
- Windows 长期 bearer credential（user token、Runner token-bearing `runner.toml`）现在与已有 Server secret 共用同一 protected secret-file primitive：当前用户 + SYSTEM FullAccess、禁用继承，ACL/reparse hardening 失败时 fail closed，overwrite 后不得退化。

发布前仍保留一次 **targeted disposable-root 安全抽检**：验证 normal execution、direct destructive deny、no-child-spawn，以及必要的 junction/reparse fail-closed 行为。它不再扩成首发前的大型安全专项；不得拿真实源码仓库或真实用户文件做 destructive 负面测试，未证实的理论 TOCTOU 风险继续记录到 Post-v1。

## 8. 当前两套本地开发/连接链路

目前机器上同时存在两套链路：

### 旧链路（暂时保留）

```text
WebCodexLauncher 0.7.2
→ WebCodex 0.3.9
→ 旧公网 Server / Runner enrollment
```

它仍承担当前网页会话中的一部分开发/维修访问能力，因此首发收敛期间先保留，不做删除或升级操作；但它不再是 v1 产品依赖，是否退休/卸载属于 Post-v1 清理。

### 新链路（CodexLink v1 dogfood）

```text
CodexLink Desktop
→ bundled CodexLink Core / Engine 0.4.1（当前 hardened code checkpoint b48b3cf1）
→ CodexLink-managed Local Server/Runner
→ OpenAI Secure Tunnel
→ ChatGPT
```

这条 Local 主链是 v1 唯一产品目标。Existing Server / Quick Share 仍存在于代码中但不进入首发 UI；`tunnel-test` 只用于 targeted 安全抽检，不再承担“必须先完成 Remote 迁移才能发布”的职责。

## 9. 当前验证证据

整改 + onboarding 收敛阶段已实际取得的主要自动化证据：

- Frontend TypeScript typecheck：通过；
- 最新 onboarding 测试线 Frontend 全部 Vitest：`70/70`，其中包含教程缩略图/lightbox、OpenAI 网络提示和 Project remove 错误分类回归；同时通过 form-control CSS contract、TypeScript typecheck 与 production build；
- Frontend production build：通过；
- 最近一次完整 Desktop Rust suite：`124 passed / 0 failed / 5 ignored`；Local zero-root runtime-anchor / ChatGPT activity identity 调整已纳入该轮验证；
- `webcodex-runner-config`：`31/31`；
- `webcodex-cli --lib`：`318/318`，其中包含 Windows credential DACL overwrite regression；
- P1 Destructive Guard focused：`7/7`；`webcodex-core` structured process encoded/shell-command-mode regression：`1/1`；tool contracts：`95/95`；
- Remote profile URL normalization / ambiguous profile / transient-vs-revoked classification、0-root、rollback、Remote readiness focused tests：通过；
- `cargo fmt --all -- --check` 与 `git diff --check`：通过；
- 从 `5ff6d8fd` 到当前 worktree 的 bounded secret scan 对 pairing/user/agent token、private-key header、OpenAI secret-key / literal assignment 均为 `0` 命中。
- commercial hardening source gate：`codexlink-product-boundary` `9/9`，其中新增 startup / second-stage 独立验签路径一致性与 tamper rejection 回归；`cargo check --locked -p webcodex -p webcodex-cli -p webcodex-runner` 通过；release-readiness 既有 `17/17` 证据保留；两个 Windows bundle/smoke PowerShell 文件 parser 既有证据保留；
- split-verifier optimized product-mode 动态 gate：合法 activation + 正确 CodexLink install layout 时 Core/Server/Runner 均穿过双 admission 后进入自身参数解析（exit 2、非 product-denied）；空 activation 时三份均 exit 78 + product-denied；三份独立 `--version` 均 exit 0；
- split-verifier optimized binary call-map：Core/Server/Runner 每份均出现两套不同的 activation verifier/material path，而不是一个 verifier 被 startup + second-stage 共享；旧 product-mode probe / marker、可读 stage/fingerprint 文本与 `CODEXLINK_BUNDLED_PRODUCT_MODE` 字符串均 `0` 命中；
- V1 `computer_*` runtime attack：`tools/list` 中 `computer_* = 0`；已知隐藏名后强制 `call_runtime_tool(computer_list_targets)` 仍返回 `unknown_tool / not_started`，不是只做 UI/list 隐藏。

**Workspace full-suite caveat：** `cargo test --workspace --lib` 在 Windows 的根 `webcodex` 巨型 test binary 中出现 suite-order `STATUS_STACK_OVERFLOW`。首次触发在 `runtime_console_http::...window_activity_lookup...`；该测试单独 `1/1` 通过且对应源码自 `5ff6d8fd` 起无 diff。串行 `--test-threads=1` 仍复现；跳过该条后又在更晚的既有 `tool_runtime::tests::jobs::list_jobs...` 发生同类 stack overflow，该条单独也 `1/1` 通过且对应源码无 diff。因此不能宣称“workspace combined suite 全绿”，也不在本轮五项整改范围外修改这些测试。其余本轮相关 focused/产品 Gate 如上均为绿色。

这些自动化证据仍不替代最终 v1 Local RC 的 clean install / restart / uninstall-reinstall / ChatGPT 真机 smoke；Remote Server 不再属于 v1 验收矩阵。

## 10. v1 Release Convergence：hardening 后剩余首发硬 Gate

项目已经停止横向扩功能。五项整改、D.1 与两步 onboarding 主体均已完成；v1 现在只保留会直接影响普通付费用户“安装后能否顺利用到第二天”的工作：

已完成的首发收敛前置项：五项整改 + D.1、两步 onboarding、首页主动作状态化、以及 Local-only v1 surface freeze。

当前剩余硬 Gate：

1. **重建并冻结 hardened 同源 v1 RC**：Desktop + bundled CodexLink Core/runtime 必须来自 commercial-hardening 之后的最终 exact source identity，记录 installer SHA / build metadata；`7ff41acd` installer 只保留为 hardening 前 evidence；v1 不要求部署同源公网 Remote Server；
2. **Local 主链生命周期验收**：激活 → onboarding → Local Engine/Runner → Secure Link → ChatGPT → project add/remove → Desktop restart → Windows restart/登录启动 → 短暂 Tunnel/Runtime 恢复，在 Launcher 关闭时仍正常；
3. **clean install / uninstall / reinstall Gate**：设备激活资格、Tunnel 凭据、授权项目与用户可恢复路径符合合同；至少一台真正 clean Windows 环境完成首次使用主链；
4. **发布前 targeted 安全抽检**：对 typed authority、credential DACL、明显 destructive deny/no-child-spawn 做 disposable-root smoke；未证实极端项记录风险，不为了理论完美阻塞首发；
5. **License Operator / 图文资料轻量收尾**：只改善真实销售所需体验，并补齐朋友真机过程中采集的截图教程，不扩成 SaaS/CRM/完整后台。

以下不再作为首个商业版本的硬 Gate：Existing Server / Remote enrollment 产品化、Quick Share、完整 Deployment Center、自动更新、macOS/Linux、Workers、多 Agent、浏览器/桌面控制、Engine 拆仓、完整 OS sandbox、机器 PKI、subscription anti-rollback、UI V2。百度网盘 / 夸克网盘 + 配套图文教程可以作为首批分发渠道。Windows code signing 强烈推荐；若第一批暂时 unsigned，必须明确标注并提供 SHA256，但不把签名基础设施本身变成无限期阻塞首发的理由。

根 `webcodex` Windows 巨型 lib test binary 的既有 suite-order stack-overflow 继续作为测试基础设施风险记录：触发用例单独运行通过、相关源码不在五项整改 diff；除非它在 RC 真实产品路径中复现，否则不升级为首发阻塞项。

## 11. v1 首发严格顺序

1. **冻结 v1 RC**，记录 Desktop / bundled CodexLink Engine exact source identity、installer SHA 与签名状态；
2. **跑 Local-only 真机生命周期**：首次安装/激活/onboarding、ChatGPT、项目 add/remove、Desktop/Windows restart、autostart、Tunnel/Runtime 恢复，且关闭 WebCodexLauncher 后仍完全可用；
3. **clean install / uninstall / reinstall + 至少一台外部 clean Windows 机器验收**；朋友真机同时采集最终教程截图；
4. **只修 release-blocking P0/P1**：数据丢失/越权、安装/激活/连接/恢复失败、频繁 crash、项目撤权失效或明显假绿导致主链不可用；其余进入 Post-v1 backlog；
5. **License Operator 小收尾，补齐图文教程/分发元数据，生成最终销售包并发布 v1。**

## 12. 操作红线

- 不删除/覆盖 `D:\Test` 历史 RC 证据；
- 不 force-push / reset published history；
- 不在安全专项完成前替换并删除唯一可工作的旧远端链路；
- 不为了刷新图标远程 kill Explorer / Shell；
- 不把 internal runtime anchor 重新当成 user authorized root；
- 不把 signed-license 协议字段单边改名；
- destructive 测试只在一次性 disposable test root 内进行；
- 任何 credential exposure、权限扩大、不可逆 migration、历史覆盖，立即 stop-the-line。
