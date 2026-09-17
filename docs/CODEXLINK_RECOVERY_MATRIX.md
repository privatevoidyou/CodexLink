# CodexLink Recovery Matrix

> 用途：把历史 `1.0.0-rc.5` 已验证能力、当前 rebuild 实现和首版剩余工作放在一张矩阵里，避免凭记忆重写或重复恢复。
>
> 当前 Engine：历史上游恢复基线 WebCodex 0.4.1 (`26c44e6436f254183d74db6865d04e2ec1d85930`)；当前 v1 dogfood 完整测试包 bundled runtime 为 `0.4.1 / 8d5a16aab50a / dirty=false`。
>
> 当前源码线：安全恢复锚点 `5ff6d8fd`；五项整改 + D.1 为 `d98eeb8b → fca59435 → ede713a0 → 112d08cc → 31ef8186`，其中远端目前停在 `31ef8186`；本地 v1 产品/测试源码 checkpoint 继续到 `b818a4ff → 62703c44 → 8d5a16aa`。后续 docs-only checkpoint 可以前进而不改变测试包 source identity；最终 v1 冻结后再统一普通 push。
>
> 首版安全策略详见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。
>
> 首版产品壳与极简配置 UX 详见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。
>
> 首版购买资格、下载与设备激活合同详见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

## 状态定义

工作类型：

- `KEEP`：当前 0.4.1 / rebuild 实现直接保留；
- `RESTORE`：从历史 RC 合同恢复；
- `HARDEN`：能力存在，但发布前要加强边界；
- `REDESIGN`：保留用户目标，但不照搬历史实现；
- `REFERENCE`：只保留研究/历史证据，不是首版产品依赖；
- `LATER`：首发不阻塞。

实施进度：`DONE` / `CHECKPOINTED` / `VERIFY` / `NEXT` / `LATER`；组合状态如 `CHECKPOINTED/VERIFY` 表示源码已固化，但仍保留真机发布验收。

## 恢复矩阵

| 区域 | 历史 RC5 / 已接受行为 | rebuild / 首版目标 | 类型 | 进度 | 主要验收 |
| --- | --- | --- | --- | --- | --- |
| Engine baseline | WebCodex 0.4.1, source `26c44e...` | 保持 exact baseline，再选择性同步上游安全修复 | KEEP | DONE | Server/Runner source aligned |
| Safe execution | 历史 RC 允许更广执行 | normal execution + Destructive Guard；CodexLink 通过 checked project activation 显式启用 | REDESIGN/HARDEN | DONE | focused Guard / activation / guidance 回归已通过；真机全链留给新 RC |
| Product name | 客户看到 CodexLink | customer-facing shell / installer 使用 CodexLink | RESTORE | DONE | fresh install 无 WebCodex 产品品牌泄漏 |
| App/runtime data | `%LOCALAPPDATA%\dev.codexlink.desktop` | 只承载普通 Desktop/runtime/connections state；设备资格独立 | RESTORE/HARDEN | WORKTREE/VERIFY | 不把 app-data 清理等同于设备解绑 |
| Device activation persistence | 设备绑定应跨普通卸载/重装保留 | `%LOCALAPPDATA%\CodexLinkSecretKey\` 保存 `device_identity.dpapi + license.json`；兼容旧目录迁移 | RESTORE/HARDEN | WORKTREE/VERIFY | 自动化已覆盖删除普通 app data 后设备资格仍存在；真实卸载/重装再验 |
| Legacy migration | bounded copy-only；旧目录保留回滚 | known-file copy-only + marker + bounded rebase | RESTORE/HARDEN | DONE | 只迁移已知文件；用户项目路径不改 |
| License | 商业设备激活 | SignedLicense + DPAPI device identity + offline verification；设备资格与 app data 分离 | RESTORE/HARDEN | WORKTREE/VERIFY | 历史 RC5 兼容 + `CodexLinkSecretKey` 迁移测试；真实 reinstall 再验 |
| Activation UI | 客户必要信息；MYWS prefix | 简化 customer copy；同码首次激活；error-code 本地化，不直出后端英文 | RESTORE | WORKTREE/VERIFY | 6 locale 品牌守卫 + 激活错误 i18n；Signed License 协议字段不单边改名 |
| Commercial entitlement portal | 激活码验证后查看下载 | 同一 `MYWS-...` 码先验证购买/下载，后用于 Desktop 设备激活；两阶段语义分清 | RESTORE/HARDEN | NEXT | valid/invalid code、下载 Gate、same-code fresh-device activation |
| Admin License Operator | 历史上人工/本地 Codex 通过 SSH 调用授权服务器签发 | Windows 本地 GUI 通过管理员 SSH key 固定调用 `issue --json`；私钥留在服务器；销售码不落盘；签发绝不自动重试 | HARDEN | CHECKPOINTED | source/installed self-test + shortcut validation + real SSH probe + 用户真实一键签发体验均已通过 |
| Product shell UI | RC5 已验收的首页/项目/连接/设置与动效心智 | RC5 心智 + Smoke 3 多项目/状态/动效/品牌收口 | RESTORE | WORKTREE READY/VERIFY | frontend 56/56；BrandFix 已出包；图标缓存后的 Explorer 最终显示仍等用户确认 |
| First-use setup UX | RC5 的项目 + Tunnel 产品心智 | 激活 → 两步 onboarding（Tunnel/Key → ChatGPT App）→ 主页面 → 按需添加多个授权路径；saved credentials 可直接继续，step 1 skip 只跳当前页；v1 不暴露其他 topology | HARDEN/RESTORE | CHECKPOINTED/USER VERIFY | `b818a4ff` + `8d5a16aa`；scroll/重开教程/readiness 分离已覆盖，截图教程继续补齐 |
| User authorized roots | 用户选哪个 root 就授权哪个 root | `projects[]` 多 root；exact canonical authority；0 root 合法；不偷偷用 HOME/anchor 扩大；精确 remove | RESTORE/HARDEN | CHECKPOINTED/VERIFY | Local/Remote 0-root + add/remove lifecycle 有自动化；父 root 仍覆盖子目录时拒绝假撤权 |
| Drive-root authorization | 支持显式选择 `C:\` / `D:\` | 可读/分析整盘；destructive target 永不允许等于 drive root | REDESIGN | VERIFY | bounded delete 的 root/wildcard 逻辑已覆盖；显式整盘授权留给新 RC 真机复验 |
| Internal runtime anchor | 历史 RC 把运行 identity 与项目心智混在一起 | Local AppData anchor 只维持 Runtime identity，不出现在项目列表、不自动成为用户 authority；Remote 直接使用 Runner identity | REDESIGN/HARDEN | CHECKPOINTED/VERIFY | 0-root snapshot 不暴露 anchor；Remote 不再需要假 enrollment project |
| Existing Server topology | 历史 Launcher 可把本机 Runner 连到远端 Server | 源码保留 `configure_remote_setup` + saved profile / transactional topology / 0-root / readiness / D.1 rollback；v1 customer surface 默认隐藏 | REFERENCE/POST-V1 | HIDDEN EXPERIMENTAL | 不阻塞 v1；未来真实需求出现后再做同源 Server + Remote lifecycle 产品化验收 |
| Quick Share | 临时共享/Provider 路径已存在于 rebuild | 底层实现保留，但 v1 不显示入口、不写入首批教程 | REFERENCE/POST-V1 | HIDDEN EXPERIMENTAL | 不阻塞 v1；以后按真实需求决定是否产品化 |
| WebCodexLauncher retirement | 旧 Launcher 0.7.2 + WebCodex 0.3.9 当前仍能工作 | 保留为开发/维修/回滚通道，不作为 v1 产品依赖 | REFERENCE | POST-V1 CLEANUP | v1 只需在 Launcher 关闭时证明 Local 主链正常；是否卸载不阻塞首发 |
| Typed create/edit/rename/delete | 历史能力不统一 | no-overwrite create/rename；SHA guarded edit/delete | HARDEN | DONE | 复用现有 canonical guarded file-edit contract，不另造重复工具 |
| Bounded recursive delete | 历史可通过 shell 广泛递归 | typed `delete_directory(path)`，只删可证明边界的明确子树 | REDESIGN | DONE | Runner focused delete 4/4；typed registry/contract Gate 已通过 |
| Raw destructive admission | 历史 shell 更自由 | recursive delete / destructive Git / mirror-purge / disk destructive + opaque PowerShell encoded payload pre-dispatch deny；不宣称 interpreter sandbox | HARDEN | CHECKPOINTED/VERIFY | Guard focused 已覆盖 encoded direct gap；真机 no-child-spawn / interpreter / reparse 动态专项待跑 |
| Windows OS sandbox | 曾做独立 POC | 不作为首版产品依赖或 release Gate | REFERENCE | LATER | `65a97891` 已普通 revert；历史研究保留 |
| Tunnel binary | OpenAI tunnel-client 0.0.12 固定 SHA | 安装包自带且 fail closed | RESTORE | DONE | SHA `664916...`; 不访问 legacy WebCodex tool cache |
| Tunnel health | 动态 `127.0.0.1:0` + url file | 保持 Engine 当前动态 health | KEEP | DONE | 不 hardcode 14286；health 200 |
| Tunnel network route | 浏览器可联网不代表 control plane 可达 | Auto 收集环境代理 → Windows 系统代理 → direct，逐候选验证 `api.openai.com`；Custom / Direct 保持确定性 | HARDEN | CHECKPOINTED/VERIFY | 不 hardcode 7890；真实机多代理证据已收集 |
| Managed self-heal | 历史版本可自动恢复大部分链路 | 启动时 + 运行中观察异常时按 desired state 恢复 Server/Runner/Tunnel；显式 stop 不自动复活 | HARDEN | WORKTREE/VERIFY | 新前端回归覆盖运行后 Tunnel 掉线；长时间真实使用继续观察 |
| Tunnel secrets | Tunnel ID + Runtime/API Key | DPAPI product-owned secret storage；不在日志回显 | RESTORE/HARDEN | DONE | logs/diagnostics 无 secret |
| Runner enrollment credentials | user token + token-bearing `runner.toml` 长期 bearer | Windows protected DACL：当前用户 + SYSTEM FullAccess，禁用继承；ACL/reparse fail closed；overwrite 不退化 | HARDEN | CHECKPOINTED | Windows focused ACL + CLI 318/318 已通过；新 build 真机再核 ACL |
| Single instance | 第二次启动不重复 runtime | 保持 | RESTORE | CHECKPOINTED/VERIFY | source regressions 已 checkpoint；normal launch + `--background` 真机继续复验 |
| Login startup | 登录时后台恢复 desired state | 保持 `runtime_autostart` + resume | RESTORE | CHECKPOINTED/VERIFY | source regressions 已 checkpoint；真机复验 no duplicate Server/Runner/Tunnel |
| Process ownership | 只结束 CodexLink 自己管理进程 | Job Object / ManagedChild；不按名字广杀 | HARDEN | DONE | 不误伤独立 WebCodex / npm Runner |
| Diagnostics | Product/Engine/Tunnel versions 分开 | customer-safe diagnostics | RESTORE/HARDEN | VERIFY | 可复制、无 credential/内部秘密 |
| Auto update | 明确不做 | 无 updater / 无检查更新网络请求 | KEEP | DONE | binary / UI 无 update path |
| Installer RC archive | accepted / rejected / archive 分离 | immutable candidate policy | KEEP | VERIFY | 新 RC 不覆盖历史 installer，记录 SHA |
| Website / tutorial content | 购买、下载、教程、FAQ、版本、许可、支持 | v1 允许百度/夸克人工分发 + 完整图文教程；Deployment Center 后续自动化同一流程，不阻塞首批 Desktop 发布 | RESTORE | V1 MINIMAL / LATER PORTAL | installer/version/SHA/激活/两步引导教程真实可跟做；OpenAI route 发版前复核 |
| UI V2 | 非首版需求 | 只有未来用户明确决定重新设计时才立项；默认延续 RC5 Product Shell | LATER | LATER | 不自动推翻已敲定首版 UI |

## 历史 RC5 固定证据

当前 `D:\Test\CodexLink\Current-RC`：

```text
CodexLink 1.0.0-rc.5
Status: UNSIGNED TEST ONLY
Product source: 0f63d08ed91c085584ebea2d3a8413d2b79af10f
WebCodex Engine: 0.4.1
Engine source: 26c44e6436f254183d74db6865d04e2ec1d85930
Installer SHA256: A7CB52ADE7000F8A82CEB29367828B56A0F6916806752F860C02364262E4756C
```

不能覆盖、重命名或重打包成同名文件。

## CodexLink state / migration contract

普通 app/runtime namespace：

```text
%LOCALAPPDATA%\dev.codexlink.desktop
```

当前这里承载可重建或普通持久状态，例如：

```text
desktop-state.json
secrets\
runtime\
connections\
```

设备激活资格已经独立：

```text
%LOCALAPPDATA%\CodexLinkSecretKey\
  device_identity.dpapi
  license.json
```

历史 `%LOCALAPPDATA%\CodexLink.DeviceActivation.v1\` 与更旧 `dev.codexlink.desktop\license\` 只作为兼容迁移输入。历史 WebCodex migration source 同样只作为迁移输入，不是新客户默认输出位置。迁移逻辑必须：

- copy-only；
- bounded known files；
- old source never delete；
- rebase only CodexLink-owned internal paths；
- preserve user project/allowed-root choices；
- migration marker prevents deleted new-state files from being resurrected。
- 设备资格迁移不覆盖不同 device identity；普通卸载/删除 app data 不应删除 `CodexLinkSecretKey`。

## Tunnel contract

历史商业 RC 的 bundled tunnel-client：

```text
version: 0.0.12
sha256: 6649169733686805CA16CCCD91774594D0C017FD729C37AD4CE1CD18323D9AE8
```

目标：

```text
<CodexLink install>\webcodex-runtime\tools\tunnel-client.exe
```

CodexLink release 不允许在 bundled client 缺失时悄悄回退到：

```text
%LOCALAPPDATA%\WebCodex\tools\...
```

缺失或 SHA mismatch 必须 fail closed，并给客户可理解的修复动作。

## Activation / commercial entitlement contract

同一枚 `MYWS-...` 销售激活码承担两个连续但不同的客户阶段：

```text
Deployment Center
  → 验证购买资格 / 查看下载
  → 不把“查看下载”伪装成设备已激活

CodexLink Desktop
  → 再次输入同一销售激活码
  → 设备绑定 / Signed License
  → 激活成功后才允许 runtime
```

RC5 已接受 Desktop copy：

- `设备激活`
- `激活 CodexLink`
- `首次使用时，请完成设备激活。`
- `销售激活码`
- placeholder 保持 `MYWS-XXXXX-XXXXX-XXXXX-XXXXX`
- `请输入购买后收到的 MYWS 激活码。`
- button `激活 CodexLink`

Deployment Center 需要明确提示：此码验证购买后仍要保留，用于 CodexLink 首次设备激活。

普通客户页面不显示 Ed25519、offline verification、DPAPI、内部授权服务细节或私有签名实现。

详细合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

## Windows real-machine regression matrix

每项恢复完成后逐步积累，不等到最后一次性测试。

| 场景 | 预期 |
| --- | --- |
| 默认安装目录 | 安装、启动正常 |
| 自定义安装目录 | registry InstallLocation 正确；升级复用用户选择 |
| Deployment Center 有效码 | 验证购买后看到正确推荐下载 / 版本 / SHA，并提示保留该码用于 Desktop 激活 |
| Deployment Center 无效码 | 不进入受保护下载入口；网络错误与无效码文案可区分 |
| 同码设备激活 | 网站验证过的同一销售激活码可在 fresh install 完成 Desktop activation |
| 首次无 license | 只出现 activation gate；后台 runtime 不偷跑 |
| 已激活 | 正常进入产品 shell |
| RC5 Product Shell | 首页/项目/连接/设置恢复已验收视觉心智，状态来自真实 backend |
| 极简首次配置 | 激活 → internal runtime 自动 bootstrap → 如缺失则保存 Tunnel ID + Runtime Key → 按需添加一个或多个授权路径 → ChatGPT App；无需理解 Server/Runner/端口 |
| invalid/recovery license | customer-level recovery copy；不暴露内部算法 |
| 添加具体项目 | exact root ready |
| 显式磁盘根 | 可读/分析；drive root 本身不可 destructive delete；明确安全子目录按 bounded policy |
| Typed create | 新文件正常；已有文件不被隐式覆盖 |
| Guarded edit | expected SHA mismatch 时 conflict，不覆盖用户新修改 |
| Typed rename | source/destination 均在授权范围；destination 已存在时拒绝 |
| Single-file delete | 明确文件正常；并发变化 / 越界拒绝 |
| Bounded recursive delete | disposable 深目录/大型 generated tree 单遍清理成功 |
| Project / drive root delete | 必须拒绝 |
| Junction / symlink / reparse delete | fail closed，不 follow target |
| Normal execution | Cargo/npm/Python/Node/PowerShell 常规开发流程可运行 |
| Destructive raw shell | recursive delete / destructive Git / mirror-purge / disk destructive 在 spawn 前拒绝 |
| Tunnel client | 使用 CodexLink bundled binary + exact SHA |
| Tunnel health | 从 url file 读取动态 loopback URL |
| Tunnel Auto route | 自动模式应选择真正可达 OpenAI control plane 的 route；不得假定 7890，也不得把第一个监听端口当成功 |
| ChatGPT | Tunnel → App → tools → read/edit/create/delete/shell 真实打通 |
| 第二实例 | 不产生重复 runtime |
| 登录时启动 | hidden launch + desired-state resume |
| Upgrade | user data/activation/project/tunnel state 保持 |
| Uninstall | program files / 普通 app data 按选择清理；`%LOCALAPPDATA%\CodexLinkSecretKey\` 设备资格不得被普通卸载语义删除 |
| Reinstall | 同一设备在设备资格仍有效时不得重新消费销售激活码；旧激活目录可迁移 |
| 独立 WebCodex 安装 | 不被 CodexLink 启停/迁移/清理误伤 |
| 外部 npm Runner | 不被 CodexLink 进程管理误伤 |
| Clean machine | 至少一台从未部署 CodexLink 的外部 Windows 机器完成首次安装主链 |

## 当前实施顺序

### Batch A — Product identity — DONE (`863d5396`)

- [x] product name / identifier；
- [x] `dev.codexlink.desktop`；
- [x] customer-facing identity boundary；
- [x] focused build/test。

### Batch B — State and migration — DONE (`b191bd85`)

- [x] state path；
- [x] bounded migration copy-only；
- [x] path rebase allowlist；
- [x] migration marker；
- [x] Windows fixture regression。

### Batch C — License and activation — DONE (`733b7b54`)

- [x] RC5 SignedLicense contract；
- [x] DPAPI device identity；
- [x] activation gate；
- [x] offline verification；
- [x] no-runtime-before-activation regression；
- [x] 历史 RC5 real state compatibility。

### Batch D — Project and Tunnel — DONE (`e85749d7`)

- [x] exact project authority；
- [x] bundled tunnel-client + fixed SHA；
- [x] DPAPI Runtime Key；
- [x] dynamic health URL；
- [x] fail-closed Tunnel handoff。

### Batch E — Lifecycle and installer — CHECKPOINTED / VERIFY (`7ebab38d`)

- [x] single instance / tray；
- [x] login/background autostart；
- [x] desired-state resume；
- [x] process ownership；
- [x] deterministic installer staging + bundled Tunnel client；
- [x] installer smoke contract；
- [ ] 真实旧版本 → 新版本 upgrade 验收（留给新 RC）；
- [x] 2026-09-15 普通 push 到 `origin/codexlink-rebuild`；未关闭 TLS 校验。

### Sandbox POC — REFERENCE (`c5fa9b50`)

- [x] host-side POC harness；
- [x] 证明当前机器 Windows Sandbox feature disabled；
- [x] 产品决策：**不作为首版依赖 / Gate**；
- [x] `65a97891` 普通 revert 从产品主线移除，保留历史证据。

### Batch F — Destructive Guard & Bounded Delete — CHECKPOINTED (`19775012`)

完整 Gate 见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。

- [x] short model safety instructions；
- [x] KEEP existing create no-overwrite / guarded edit expected-SHA / rename no-overwrite / single-file delete contracts；
- [x] `delete_directory` bounded recursive delete；
- [x] lightweight preflight + exact project-relative target validation；
- [x] root / parent / wildcard / `.git` / protected-anchor guard；
- [x] reparse/junction/symlink fail-closed；
- [x] single-pass deletion + bounded summary；
- [x] raw recursive-delete admission；
- [x] destructive Git admission；
- [x] mirror/purge + disk-destructive admission；
- [x] Guard denial只返回固定 reason code/copy，不把 raw command 写入新审计字段；
- [x] normal execution 恢复走 `project activate --allow-raw-shell` + config check/generation CAS/hot reload；
- [x] focused evidence：Guard 7/7、Runner structured delete 4/4、CLI activation 11/11、startup guidance 4/4、tool contracts 95/95、tool-runtime-contracts 73/73、registry structured delete 10/10、file validation 6/6；
- [ ] 新 RC 真机复验大型 generated tree、junction/reparse、normal Cargo/npm/Python/Node/PowerShell 与拒绝后无 child spawn。

### Batch G — RC5 Product Shell & Minimal Setup UX — WORKTREE READY / REAL-MACHINE VERIFY

完整 UI 契约见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。

- [x] 首页恢复“一切就绪 + 项目大卡片 + 连接链路 + 状态摘要”；
- [x] 首次无 License 时继续由现有 RC5 简洁 Activation Gate 拦截；激活成功前不进入可工作 Product Shell；
- [x] 项目页恢复卡片式项目管理；
- [x] 连接页恢复步骤链 + 当前状态 + Secure Tunnel 卡片；Remote / Quick Share 底层/历史页面实现保留，但 v1 surface freeze 后不向普通用户展示；
- [x] 设置页收敛为语言 / 登录启动 / Tunnel ID / Restricted Runtime Key / 高级网络；
- [x] 保存 Tunnel 凭据后，在本地 runtime 已就绪时自动停止旧 regular tunnel 并重新准备 Secure Link；
- [x] 普通用户首用不暴露 Server URL / Runner enrollment / loopback port / tunnel-client path；激活后 internal runtime 自动 bootstrap，项目 authority 可随后按需添加；
- [x] Smoke 2 历史 Gate：Vitest 51/51、Rust 串行 105/105 等已记录；
- [x] internal runtime anchor 与 `projects[]` user authority 分层；0 root 时 Runtime/transport 可连接、ChatGPT transport 可显示“可访问”，但 aggregate `ready_for_chatgpt=false`，整体仍等待 customer-authorized project；
- [x] 多 root add/remove authority；Remote add/remove 不切 Local、不重新 pairing；
- [x] Remote Server/Runner readiness 与 Local OpenAI Tunnel 投影分离；offline URL 不假绿；
- [x] UIFix / BrandFix：RC5 心智渐变/动效、Tray 精简/locale、激活错误 i18n、6 locale 品牌守卫；
- [x] 本轮 Stage D 自动化：frontend typecheck + focused `49/49`；Rust Desktop `121 passed / 0 failed / 5 ignored`；Runner config `31/31`；CLI lib `318/318`；fmt / diff check 通过；
- [x] 历史 BrandFix installer SHA256：`B67AB5388095EAB2A256AA2E9EF94231A94E87FE8ED7EBCB939F0D9159C8FF71`；bundled runtime `0.4.1 / ae858f5360ff / dirty=false`；当前 v1 dogfood 已继续推进到 `8d5a16aa`。
- [ ] Explorer 图标缓存后的最终用户确认；设备激活跨卸载真实复验；长时间断线自愈观察；
- [x] 安全整改代码已拆为 `d98eeb8b` / `fca59435` / `ede713a0` 三个 checkpoint；Stage E 文档、回归、secret scan 与 push 已在 `48f9af16` 收口并固定为 final remediation HEAD。后续纯文档 v1 规划不改变该审计边界。

### Batch H.0 — Local License Operator — USER VERIFIED / CHECKPOINT

完整管理员合同见 [`CODEXLINK_LICENSE_OPERATOR.md`](CODEXLINK_LICENSE_OPERATOR.md)。

- [x] Windows 本地 GUI；
- [x] SSH private-key path only，不复制私钥；
- [x] 固定远端 `/usr/local/bin/myws-license-admin issue --json`；
- [x] 严格解析唯一 `MYWS-...` JSON；
- [x] 成功后显示并复制剪贴板，不保存销售码历史；
- [x] `issue` 非幂等，任何失败路径都不自动重试；
- [x] Windows PowerShell 5.1 UTF-8 BOM 兼容；
- [x] 安装副本 self-test / shortcut / icon / config / SSH probe；
- [x] 2026-09-15 用户人工点击 GUI 完成一次实际销售码签发体验验收；
- [ ] 服务端未来如增加 `list / inspect / revoke`，再另批扩展，不直接读写 `licenses.db`。

### Safety remediation / advanced topology — SOURCE COMPLETE; v1 TARGETED VERIFY

- [x] arbitrary execution capability contract 与 encoded PowerShell direct gap；
- [x] Windows Runner/user long-lived credential protected DACL；
- [x] Remote saved profile / transactional topology / 0-root / real readiness + D.1 replacement/rollback 源码整改；
- [ ] v1 RC 在 disposable test root 做 targeted normal execution + destructive deny + no-child-spawn + 必要 reparse fail-closed smoke；
- [ ] Existing Server / Quick Share 的公网部署、Remote lifecycle 与 Launcher retirement 产品化验收全部转 Post-v1，不阻塞首版。

### Batch H — v1 First-Use Onboarding — CHECKPOINTED / USER VERIFY

完整 UI/商业合同见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md) 与 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

- [x] 激活成功后的首次用户进入两步 onboarding，而不是直接首页；
- [x] 第 1 步：Tunnel ID + Restricted Runtime Key + 官方创建入口；saved credentials 可直接继续；
- [x] 第 1 步 `我已经配置过，跳过` 只跳当前页进入第 2 步；
- [x] 第 2 步：ChatGPT App 创建/添加文字教程；可滚动、可在设置页重新打开；
- [x] onboarding 完成/跳过不改变真实 readiness；
- [ ] 补齐最终真实截图教程，并让百度网盘 / 夸克网盘资料与软件内引导保持同一客户主链。

### Batch I — v1 Surface Freeze + Local-only RC

- [x] 五项整改与 D.1 独立复审完成；没有已知 release-blocking security P0/P1；
- [x] 最后一轮用户体验反馈已收口：首页显著主按钮按真实状态切换为恢复连接 / 启动 / 添加项目 / 查看项目，并复用既有安全恢复路径；
- [x] 正式 App 路径隐藏 Existing Server、Quick Share、模式选择页、onboarding 高级连接入口和 `其他配置方式`；底层实现与 focused tests 保留；
- [ ] unique new RC，Desktop + bundled CodexLink Engine exact source/release identity 对齐；
- [ ] installer SHA + release metadata；
- [ ] Local 主链跑 activation/onboarding/0-root/project add-remove/ChatGPT/read-write/normal execution/Desktop+Windows restart/Tunnel recovery；
- [ ] 关闭 WebCodexLauncher 后 Local 主链仍完全可用；Launcher 是否卸载不阻塞 v1；
- [ ] upgrade/uninstall/reinstall；
- [ ] 至少一台从未安装 CodexLink 的朋友/外部测试机 clean-install；
- [ ] 只修阻塞首发的 P0/P1，不覆盖历史 artifact。

### Batch J — First commercial release

- [ ] 人工分发包（百度/夸克或等价渠道）包含 installer、version、SHA256 与图文教程；
- [ ] License Operator 只做真实销售需要的轻量体验收尾；
- [ ] third-party notices / privacy / security / support 文案满足最低发布要求；
- [ ] Windows code signing 强烈推荐；若首批 unsigned，明确标注并提供 SHA256，不因签名基础设施无限期拖延发布；
- [ ] final clean-machine acceptance；
- [ ] archive / v1 commercial release；
- [ ] 完整 Deployment Center、自动更新、跨平台、Workers/多 Agent、UI V2 等全部进入 Post-v1 backlog。

## 规则

- 每一批只处理一个清晰产品层；
- 不因为“旧 RC 以前能跑”就跳过当前 0.4.1 的 focused tests；
- 普通 shell/process/script 是首版正常能力，不因为解释器类型频繁打断用户；
- destructive filesystem change 优先用 typed operation；raw recursive delete 由 Guard 拒绝；
- Bounded Recursive Delete 允许深目录/大目录，但必须能证明明确 target boundary；
- 不把 Git 作为删除安全的硬依赖；
- 不把 prompt / blacklist 宣传成 OS sandbox；
- 不覆盖 accepted/rejected RC；
- 不删除历史证据目录；
- `D:\Test` 的 Current-RC / Archive / Rejected / Rollback / CodexLink-Test 是首版 UI 与行为证据，只读保留；
- 首版产品壳以已验收 RC5 为权威基准，不因当前 rebuild frontend 已存在就默认沿用 rebuild Dashboard；
- 普通用户配置优先减少步骤和概念；v1 用户表面只保留 Local + OpenAI Tunnel + ChatGPT，Server/Runner/端口/客户端路径自动隐藏，Existing Server / Quick Share / 其他 topology 入口默认不展示；
- 销售激活码是购买 entitlement 与 Desktop 首次激活凭证，不降级成单纯下载密码；
- Deployment Center 验证购买与 Desktop 设备激活必须使用同一条客户主线，但不能混淆“查看下载”和“设备已激活”；
- 不把旧 WebCodex Desktop frontend 当成 CodexLink 产品 UI 的长期上游；
- 遇到不可逆 migration / credential exposure / 未受控 destructive scope，先停线评审。
