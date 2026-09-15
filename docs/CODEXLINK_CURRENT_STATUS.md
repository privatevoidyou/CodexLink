# CodexLink Current Status

> 状态快照：**2026-09-15，Smoke 3 / Product Shell 收口阶段。**
>
> 用途：记录当前 `codexlink-rebuild` 工作树的真实状态、已验证能力、尚未完成 Gate 和下一步顺序。本文优先描述“现在是什么”，长期产品合同仍分别以 Security Boundary、First-Release UI、Commercial Flow 等专题文档为准。

## 1. Git / source snapshot

- 仓库：`D:\GitHub\CodexLink`
- 分支：`codexlink-rebuild`
- 当前已提交并已推远端 HEAD：`4b4043ca1d1d40363e474c2d727834a70d48b631` — `CodexLink: add local license operator`
- `origin/codexlink-rebuild` 当前与该 HEAD 对齐。
- 当前 Smoke 3 / AnchorFix / UIFix / RecoveryFix / BrandFix 仍位于**未提交工作树**：在本轮文档同步之前，产品源码本身已有 35 个 tracked 文件 diff，并有新的 Windows 图标/installer hook 资源；本轮又增加 10 份已有文档修改 + 1 份新的 Current Status。不要把这些未提交改动误称为远端 checkpoint。
- 当前本地测试包：`target\CodexLink_0.4.1_RC5-BrandFix_x64-setup.exe`
- BrandFix installer SHA256：`B67AB5388095EAB2A256AA2E9EF94231A94E87FE8ED7EBCB939F0D9159C8FF71`
- 当前 bundled WebCodex runtime：`0.4.1 / ae858f5360ff / dirty=false / built_at=1789473127`（CLI / Server / Runner 三者一致）。

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
- 0 个用户项目也允许 Engine / Secure Link / ChatGPT 连接处于健康状态，UI 只提示“等待配置授权路径”。

当前高级“连接现有 Server”页面仍会把 internal anchor 作为 enrollment project/allowed-root 展示出来，这是**已知的产品层清理项**；不影响本机主链，但在 Launcher 退休前应隐藏或改为内部说明，避免用户把它当成业务项目。

### 2.2 本机与 Remote Server 是两种不同拓扑

- 默认本机模式：CodexLink 管理本机 Server + Runner + OpenAI Secure Tunnel。
- Existing Server 模式：远端保留 Server，CodexLink Desktop 只 enrollment/监管本机 Runner。
- Remote 模式已经有真实 `configure_remote_setup` 实现：一次性 `wc_pair_...`、保存 enrollment、Server readiness、Runner lifecycle、后续 identity reuse 都不是空 UI。
- Remote 模式会检查 Server revision；远端 Server 与当前 Desktop/runtime 不匹配时应 fail closed，而不是勉强混跑。

因此旧 WebCodexLauncher **现在还不能删除**：公网旧链路仍是 Launcher `0.7.2` + WebCodex `0.3.9`；等服务器部署为与 CodexLink 同源的 fork build，并完成 Remote 模式真实 smoke 后再退休。

## 3. Product Shell / UI 状态

Smoke 3 期间已经从“恢复 RC5 大概样子”推进到产品语义和交互基本收口：

- 主导航固定为 `首页 / 项目 / 连接 / 设置`；顶部“当前工作区”已从普通页面删除；
- 首页主卡只显示第一个用户授权路径；0 项目时显示“等待配置授权路径”；
- 首页连接链路 `本机 → Engine → Secure Link → ChatGPT` 与“是否已添加项目”分离；链路健康时 0 项目也保持绿色；
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

Batch F 已提交的基础能力保持不变：

- normal shell/process/script execution 可用；
- typed create/edit/rename/delete 保持 no-overwrite / expected-SHA / exact-root 合同；
- bounded recursive `delete_directory`；
- project/root/parent/wildcard/protected anchor/reparse fail closed；
- raw recursive delete、destructive Git、mirror/purge、disk destructive primitive 在 spawn 前拒绝；
- 不声称这是 OS sandbox；解释器内部恶意脚本仍是明确非目标。

当前 CodexLink 0.4.1 / `ae858f...` 通过 `tunnel-test` 已经证明 execution interface 会区分结构化 process 与 shell：把 PowerShell shell 语义塞进 `run_process` 会被预执行拒绝，必须走专用 shell 路径。

**下一项最高优先级安全 Gate**：使用 `tunnel-test` 在专门 disposable test root 内做 destructive shell 端到端测试，重点证明拒绝发生在 child spawn 前，并覆盖 PowerShell / cmd / shell 常见高风险形式。不得拿真实源码仓库或真实用户文件做 destructive 负面测试。

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

当前工作树最近一轮 release-level 验证记录：

- Frontend TypeScript：通过；
- Frontend Vitest：`56/56`；
- production build：通过；
- Rust Desktop：`114 passed / 0 failed / 5 ignored`；
- `cargo fmt --check`：通过；
- `git diff --check`：通过；
- BrandFix NSIS release：成功；
- bundled runtime：CLI / Server / Runner 均为 `0.4.1 / ae858f5360ff / dirty=false`；
- installer 内 Shell icon refresh hook 已进入最终 NSIS；
- 最终 EXE 32px associated icon 与当前 CodexLink 蓝色资源字节一致。

这些测试证明当前源码/包的自动化 Gate，不替代下面仍待完成的真实系统验收。

## 10. 仍未完成 / 不应误报完成

1. **Destructive Guard 的 tunnel-test 真机专项验收**尚未完成；
2. **公网 Server 还没有部署成当前 CodexLink fork build**；
3. **Existing Server 模式还没有在“同源新 Server”上完成一次真实 pairing → restart → reuse smoke**；
4. **WebCodexLauncher 还不能退休**；
5. BrandFix 的**设备激活跨卸载真实复验**仍应再跑一次；
6. Explorer 图标显示在缓存清理后的**最终用户确认**仍待收口；
7. Runner / Tunnel 长时间运行中的**真实意外断线自愈**继续观察；
8. 高级 Remote Server 页面仍暴露 internal runtime anchor，需产品化隐藏/说明；
9. Smoke 3 / UIFix / RecoveryFix / BrandFix 的 35-file 产品源码 diff，加上本轮文档同步，**尚未 commit / push**；
10. Deployment Center 商业 MVP、code signing、third-party notices、外部 clean-machine acceptance 仍未完成。

## 11. 下一阶段严格顺序

建议继续按以下顺序推进，避免再把风险面混在一起：

1. **文档与当前状态同步**（本轮）；
2. **tunnel-test 安全专项**：正常开发命令 + destructive deny + no-child-spawn；
3. 安全 Gate 通过后，让本地 Codex 按固定 source identity 把公网 Server 部署为当前 CodexLink fork；
4. 用 CodexLink Existing Server 模式完成一次真实 `wc_pair_...` enrollment；
5. 退出/重启 CodexLink，确认 enrollment reuse，不再次要求 pairing；
6. 用 `tunnel-test` / ChatGPT 通过远端 Server 做 read/write/normal execution/Guard smoke；
7. 上述链路稳定后，**再退休 WebCodexLauncher 0.7.2 / WebCodex 0.3.9**；
8. 整理 Smoke 3 大 worktree：review diff、补最终文档、拆合理 checkpoint、commit/push；
9. 宣布进入**规模测试阶段**：多项目、不同代理、断线恢复、upgrade/uninstall/reinstall、外部 clean-machine；
10. 最后收口 Deployment Center、code signing、third-party notices 与 Stable release。

## 12. 操作红线

- 不删除/覆盖 `D:\Test` 历史 RC 证据；
- 不 force-push / reset published history；
- 不在安全专项完成前替换并删除唯一可工作的旧远端链路；
- 不为了刷新图标远程 kill Explorer / Shell；
- 不把 internal runtime anchor 重新当成 user authorized root；
- 不把 signed-license 协议字段单边改名；
- destructive 测试只在一次性 disposable test root 内进行；
- 任何 credential exposure、权限扩大、不可逆 migration、历史覆盖，立即 stop-the-line。
