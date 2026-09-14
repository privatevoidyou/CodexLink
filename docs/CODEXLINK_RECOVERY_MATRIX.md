# CodexLink Recovery Matrix

> 用途：把历史 `1.0.0-rc.5` 已验证能力、当前 rebuild 实现和首版剩余工作放在一张矩阵里，避免凭记忆重写或重复恢复。
>
> 当前 Engine 基线：WebCodex 0.4.1 (`26c44e6436f254183d74db6865d04e2ec1d85930`)。
>
> 当前产品 checkpoint：Safe Mode `22cab846` → Identity `863d5396` → State/Migration `b191bd85` → License `733b7b54` → Project/Tunnel `e85749d7` → Lifecycle/Installer `7ebab38d`（本地）→ Sandbox POC `c5fa9b50`（研究实验，待普通 revert）。
>
> 首版安全策略详见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。
>
> 首版产品壳与极简配置 UX 详见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。

## 状态定义

工作类型：

- `KEEP`：当前 0.4.1 / rebuild 实现直接保留；
- `RESTORE`：从历史 RC 合同恢复；
- `HARDEN`：能力存在，但发布前要加强边界；
- `REDESIGN`：保留用户目标，但不照搬历史实现；
- `REFERENCE`：只保留研究/历史证据，不是首版产品依赖；
- `LATER`：首发不阻塞。

实施进度：`DONE` / `LOCAL` / `NEXT` / `VERIFY` / `LATER`。

## 恢复矩阵

| 区域 | 历史 RC5 / 已接受行为 | rebuild / 首版目标 | 类型 | 进度 | 主要验收 |
| --- | --- | --- | --- | --- | --- |
| Engine baseline | WebCodex 0.4.1, source `26c44e...` | 保持 exact baseline，再选择性同步上游安全修复 | KEEP | DONE | Server/Runner source aligned |
| Safe execution | 历史 RC 允许更广执行 | Batch F 通过后恢复 normal execution + Destructive Guard | REDESIGN/HARDEN | NEXT | 正常 build/test/script 通过；obvious destructive shell pre-spawn 拒绝 |
| Product name | 客户看到 CodexLink | customer-facing shell / installer 使用 CodexLink | RESTORE | DONE | fresh install 无 WebCodex 产品品牌泄漏 |
| App data | `%LOCALAPPDATA%\dev.codexlink.desktop` | 产品独立 namespace | RESTORE | DONE | clean profile 只创建 CodexLink namespace |
| Install/data split | 程序目录可自定义；用户状态独立持久 | upgrade/uninstall 不误删持久状态 | RESTORE | LOCAL | installer smoke 已完成；真实 upgrade 留给新 RC |
| Legacy migration | bounded copy-only；旧目录保留回滚 | known-file copy-only + marker + bounded rebase | RESTORE/HARDEN | DONE | 只迁移已知文件；用户项目路径不改 |
| License | 商业设备激活 | RC5 SignedLicense + DPAPI device identity + offline verification | RESTORE/HARDEN | DONE | 历史 RC5 状态真实验证通过 |
| Activation UI | 客户必要信息；MYWS prefix | 保持简化 customer copy | RESTORE | DONE | production bundle 不含不必要实现说明 |
| Product shell UI | RC5 已验收的首页/项目/连接/设置与动效心智 | 首版恢复 RC5 Product Shell，不采用当前 rebuild Dashboard 作为最终 UI | RESTORE | NEXT | 逐页视觉/状态/交互与真实 RC 对照 |
| First-use setup UX | RC5 已接近“项目 + Tunnel”产品心智 | 激活 → 选项目 → 保存 Tunnel ID + Runtime Key → ChatGPT App；底层自动处理 | HARDEN/RESTORE | NEXT | 普通用户无需 Server/Runner/端口/客户端路径配置 |
| Project root | 用户选哪个 root 就授权哪个 root | exact canonical root；不偷偷扩大 | RESTORE/HARDEN | DONE | normal dir/root/UNC/junction regressions |
| Drive-root authorization | 支持显式选择 `C:\` / `D:\` | 可读/分析整盘；destructive target 永不允许等于 drive root | REDESIGN | NEXT | 明确子目录可 bounded delete；root 本身拒绝 |
| Typed create/edit/rename/delete | 历史能力不统一 | no-overwrite create/rename；SHA guarded edit/delete | HARDEN | NEXT | 并发变化 fail closed |
| Bounded recursive delete | 历史可通过 shell 广泛递归 | typed `delete_directory(path)`，只删可证明边界的明确子树 | REDESIGN | NEXT | deep tree / large generated tree / root / wildcard / reparse regressions |
| Raw destructive admission | 历史 shell 更自由 | recursive delete / destructive Git / mirror-purge / disk destructive pre-spawn deny | HARDEN | NEXT | denied request 不产生 child process |
| Windows OS sandbox | 曾做独立 POC | 不作为首版产品依赖或 release Gate | REFERENCE | LATER | 保留历史研究；主线普通 revert POC |
| Tunnel binary | OpenAI tunnel-client 0.0.12 固定 SHA | 安装包自带且 fail closed | RESTORE | DONE | SHA `664916...`; 不访问 legacy WebCodex tool cache |
| Tunnel health | 动态 `127.0.0.1:0` + url file | 保持 Engine 当前动态 health | KEEP | DONE | 不 hardcode 14286；health 200 |
| Tunnel secrets | Tunnel ID + Runtime/API Key | DPAPI product-owned secret storage；不在日志回显 | RESTORE/HARDEN | DONE | logs/diagnostics 无 secret |
| Single instance | 第二次启动不重复 runtime | 保持 | RESTORE | LOCAL | normal launch + `--background` |
| Login startup | 登录时后台恢复 desired state | 保持 `runtime_autostart` + resume | RESTORE | LOCAL | no duplicate Server/Runner/Tunnel |
| Process ownership | 只结束 CodexLink 自己管理进程 | Job Object / ManagedChild；不按名字广杀 | HARDEN | DONE | 不误伤独立 WebCodex / npm Runner |
| Diagnostics | Product/Engine/Tunnel versions 分开 | customer-safe diagnostics | RESTORE/HARDEN | VERIFY | 可复制、无 credential/内部秘密 |
| Auto update | 明确不做 | 无 updater / 无检查更新网络请求 | KEEP | DONE | binary / UI 无 update path |
| Installer RC archive | accepted / rejected / archive 分离 | immutable candidate policy | KEEP | VERIFY | 新 RC 不覆盖历史 installer，记录 SHA |
| Website | 购买、下载、教程、FAQ、版本、许可、支持 | 完整商业产品站 + 安全边界说明 | RESTORE | LATER | 真实截图 + current OpenAI routes |
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

## 历史 CodexLink state / migration contract

预期产品 namespace：

```text
%LOCALAPPDATA%\dev.codexlink.desktop
```

历史已接受内容：

```text
desktop-state.json
license\
secrets\
runtime\
connections\
```

历史 WebCodex migration source 仅作为迁移输入，不是新客户默认输出位置。迁移逻辑必须：

- copy-only；
- bounded known files；
- old source never delete；
- rebase only CodexLink-owned internal paths；
- preserve user project/allowed-root choices；
- migration marker prevents deleted new-state files from being resurrected。

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

## Activation contract

RC5 已接受 copy：

- `设备激活`
- `激活 CodexLink`
- `首次使用时，请完成设备激活。`
- `销售激活码`
- placeholder 保持 `MYWS-XXXXX-XXXXX-XXXXX-XXXXX`
- `请输入购买后收到的 MYWS 激活码。`
- button `激活 CodexLink`

普通客户页面不显示：

- Ed25519；
- offline verification 内部实现；
- plaintext-storage 实现细节；
- 内部授权服务细节；
- 旧 download portal。

这只是信息最小化，不等于安全边界。真正的安全仍来自 private signing key 不出客户端、签名验证、secret protection、signed binaries、最小诊断泄漏。

## Windows real-machine regression matrix

每项恢复完成后逐步积累，不等到最后一次性测试。

| 场景 | 预期 |
| --- | --- |
| 默认安装目录 | 安装、启动正常 |
| 自定义安装目录 | registry InstallLocation 正确；升级复用用户选择 |
| 首次无 license | 只出现 activation gate；后台 runtime 不偷跑 |
| 已激活 | 正常进入产品 shell |
| RC5 Product Shell | 首页/项目/连接/设置恢复已验收视觉心智，状态来自真实 backend |
| 极简首次配置 | 激活 → 选项目 → 设置保存 Tunnel ID + Runtime Key → ChatGPT 创建/添加 App；无需理解 Server/Runner/端口 |
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
| ChatGPT | Tunnel → App → tools → read/edit/create/delete/shell 真实打通 |
| 第二实例 | 不产生重复 runtime |
| 登录时启动 | hidden launch + desired-state resume |
| Upgrade | user data/activation/project/tunnel state 保持 |
| Uninstall | program files 删除策略正确；持久数据按产品策略保留 |
| Reinstall | 保留策略与历史状态可解释、可恢复 |
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

### Batch E — Lifecycle and installer — LOCAL DONE (`7ebab38d`)

- [x] single instance / tray；
- [x] login/background autostart；
- [x] desired-state resume；
- [x] process ownership；
- [x] deterministic installer staging + bundled Tunnel client；
- [x] installer smoke contract；
- [ ] 真实旧版本 → 新版本 upgrade 验收（留给新 RC）；
- [ ] GitHub TLS 恢复后的普通 push。

### Sandbox POC — REFERENCE (`c5fa9b50`)

- [x] host-side POC harness；
- [x] 证明当前机器 Windows Sandbox feature disabled；
- [x] 产品决策：**不作为首版依赖 / Gate**；
- [ ] 用普通 revert 从产品主线移除，保留历史证据。

### Batch F — Destructive Guard & Bounded Delete — NEXT

完整 Gate 见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。

- [ ] short model safety instructions；
- [ ] create no-overwrite；
- [ ] guarded edit / expected SHA regression；
- [ ] rename no-overwrite；
- [ ] `delete_file`；
- [ ] `delete_directory` bounded recursive delete；
- [ ] lightweight preflight；
- [ ] root / parent / wildcard / protected-anchor guard；
- [ ] reparse/junction/symlink fail-closed；
- [ ] single-pass deletion + bounded summary；
- [ ] raw recursive-delete admission；
- [ ] destructive Git admission；
- [ ] mirror/purge + disk-destructive admission；
- [ ] safe audit / diagnostics leakage regression；
- [ ] normal shell/process/script focused tests；
- [ ] 全部通过后才切换 execution policy。

### Batch G — RC5 Product Shell & Minimal Setup UX — NEXT AFTER F

完整 UI 契约见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)。

- [ ] 首页恢复“一切就绪 + 项目大卡片 + 连接链路 + 状态摘要”；
- [ ] 项目页恢复卡片式项目管理；
- [ ] 连接页恢复步骤链 + 当前状态 + Secure Tunnel 卡片；
- [ ] 设置页收敛为语言 / 登录启动 / Tunnel ID / Restricted Runtime Key / 高级网络；
- [ ] 保存 Tunnel 凭据后自动准备或恢复所需 runtime/tunnel；
- [ ] 普通用户首用不暴露 Server URL / Runner enrollment / loopback port / tunnel-client path；
- [ ] 用 `D:\Test\CodexLink\Current-RC` 与真实安装程序逐页对照视觉、滚动、hover、focus、状态动效。

### Batch H — New RC / real-machine acceptance

- [ ] unique new RC；
- [ ] installer SHA + release metadata；
- [ ] 本机完整 Windows 主链；
- [ ] 至少一台从未安装 CodexLink 的朋友/外部测试机 clean-install；
- [ ] 验证朋友可以按教程完成“激活 → 项目 → 两项 Tunnel 凭据 → ChatGPT App”，不需要解释底层 runtime；
- [ ] upgrade/uninstall/reinstall；
- [ ] 收集问题、修复并重新候选，不覆盖历史 artifact。

### Batch I — Commercial readiness

- [ ] website/tutorial + security boundary copy；
- [ ] code signing；
- [ ] third-party notices；
- [ ] final clean-machine acceptance；
- [ ] archive / stable release。

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
- 普通用户配置优先减少步骤和概念，Server/Runner/端口/客户端路径等高级实现默认自动隐藏；
- 不把旧 WebCodex Desktop frontend 当成 CodexLink 产品 UI 的长期上游；
- 遇到不可逆 migration / credential exposure / 未受控 destructive scope，先停线评审。
