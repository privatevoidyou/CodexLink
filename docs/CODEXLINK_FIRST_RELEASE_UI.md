# CodexLink First-Release Product Shell & Setup UX

> 状态：**2026-09-15 首版 UI / UX 基线已通过 Smoke 3 多轮真机收敛，产品心智基本冻结；当前实现仍在未提交 worktree，剩余以真实系统验收和小范围修正为主。**
>
> 核心决策：**首版沿用已验收 RC5 成品的产品壳、信息层级、状态表达和交互风格，不再为首发另起一套 Dashboard 视觉方向。**
>
> 实施状态：**`cafc0b7e` + `09c48060` 是已提交基线；其后 Smoke 3 / AnchorFix / UIFix / RecoveryFix / BrandFix 已完成多项目 authority、internal runtime anchor 分离、状态语义、动效、i18n 与安装/恢复迭代，尚未形成新的 Git checkpoint。实时状态见 `CODEXLINK_CURRENT_STATUS.md`。**
>
> 目标：用户只理解“项目”和“ChatGPT 连接”，Server、Runner、端口、进程、Tunnel client 等实现细节由 CodexLink 自动处理。

本文是首版 CodexLink 产品壳与首次配置体验的权威规范。安全边界仍以 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md) 为准；购买资格与设备激活合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

---

## 1. 已冻结的产品方向

首版 UI 以历史 `CodexLink 1.0.0-rc.5` 成品为视觉和交互基准。

保留其核心心智：

- 左侧固定产品导航：`首页 / 项目 / 连接 / 设置`；
- 普通页面顶部不再显示“当前工作区”；全局状态只在页面自身的产品区域表达；
- 首页以“一切就绪”或明确的下一步为主，而不是展示底层 runtime 术语；
- 首页使用大卡片表达第一个授权路径；多个授权路径不拼成长标题，0 个路径时显示“等待配置授权路径”；
- `本机 → Engine → Secure Link → ChatGPT` 使用清晰的连接链路表达；
- “接下来做什么”与“系统状态”只给用户需要的动作和状态；
- 项目页使用卡片式项目管理；
- 连接页使用步骤链说明运行环境、Tunnel 凭据、安全链路与 `ChatGPT 可访问`；连接健康不再依赖“是否已经发生过一次真实工具调用”；
- 设置页集中承载语言、开机启动、Tunnel 凭据和必要的网络高级设置；
- 绿色表示已就绪/连接正常，危险动作使用明确但克制的红色；
- 保留 RC 成品中自然的 hover、状态点、卡片和页面切换动效节奏，不另外引入夸张动画体系。

首版不以当前 rebuild Dashboard 的信息架构作为最终 UI，也不在发布前启动一套新的“UI V2”视觉探索。

---

## 2. 历史证据与源码恢复状态

当前可信证据：

1. `D:\Test\CodexLink\Current-RC`：`1.0.0-rc.5` installer、release metadata、runtime stage；
2. `D:\Test\CodexLink-Test\CodexLink`：真实安装形态的 `CodexLink.exe` 与 bundled runtime；
3. `D:\Test\CodexLink\Archive`：已接受历史 RC；
4. `D:\Test\CodexLink\Rejected`：已拒绝候选及其字节证据；
5. `D:\Test\CodexLink\Rollback`：旧稳定回滚候选；
6. 本项目会话中保存的 RC5 首页、项目、连接、设置截图；
7. release metadata 中记录的 RC5 Product source：`0f63d08ed91c085584ebea2d3a8413d2b79af10f`。
8. `D:\Test\CodexLink-Test\CodexLink\CodexLink.exe` 中的 Windows icon resources：2026-09-15 已从真实 RC5 EXE 提取 16/24/32/48/64/128/256px 图标，作为当前产品图标恢复的字节级证据。

当前已核实：

- `D:\GitHub\CodexLink-stable-v1` 只剩目录骨架，没有可直接恢复的前端源码；
- RC5 product source commit `0f63d08e...` 当前不在 `D:\GitHub\CodexLink` Git object database 中；
- `D:\Test\CodexLink-LinuxBuild-22cab` 有完整 TSX/CSS，但属于后来 rebuild UI，不是截图中的 RC5 产品壳；
- `D:\Test` 中暂未发现截图对应 RC5 UI 的独立 TSX/CSS/source-map 副本。

因此恢复优先级固定为：

> 若以后找到真实 RC5 UI 源码，优先复用；否则以 RC5 真实程序 + 截图 + 行为观察为准，在当前 React/Tauri 架构内恢复，不凭空 redesign。

所有 `D:\Test` 历史目录继续只读保留，不因首版恢复而清理或覆盖。

---

## 2.5 首次启动 Activation Gate

RC5 Product Shell 之前必须先经过商业设备激活状态机。

用户从 Deployment Center 下载并安装后，首次打开看到的是简洁激活页，而不是直接进入首页：

```text
设备激活
激活 CodexLink
首次使用时，请完成设备激活。

销售激活码
[ MYWS-XXXXX-XXXXX-XXXXX-XXXXX ]

[ 激活 CodexLink ]
```

要求：

- 使用购买后收到的**同一枚** `MYWS-...` 销售激活码；
- 未激活时不启动 Server / Runner / Tunnel runtime；
- 激活成功后才进入 Product Shell，并由 CodexLink 使用 internal runtime anchor 自动 bootstrap；用户授权路径不作为启动 Runtime 的前置条件；
- 客户页面不显示签名算法、DPAPI、Device ID、内部授权 endpoint 等实现细节；
- 激活失败只给客户可理解的重试 / 支持动作，不提供绕过 Activation Gate 的高级开关。

详细商业合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

---

## 3. 首页契约

首页回答三个问题：

1. 本机到 ChatGPT 的链路现在能不能用；
2. 当前是否已有用户授权路径，若有则第一个授权路径是什么；
3. 如果尚未授权路径，下一步在哪里添加。

**重要：连接健康与项目数量分离。** Engine / Secure Link / ChatGPT 已就绪时，即使 `projects.length === 0`，连接链路也保持绿色；项目数量只表达本地 authority，而不是连接故障。

目标结构：

```text
首页                                      [总状态] [刷新]
一切就绪 / 再完成一步即可使用

[授权路径大卡片]
第一个授权路径，或“等待配置授权路径”
AI 连接 / 运行环境 / 项目数量
查看/添加项目 | 连接详情

[连接链路]
本机 ───── Engine ───── Secure Link ───── ChatGPT

[接下来做什么]        [系统状态]
查看/添加项目          CodexLink 服务
管理连接              Runner
                      项目
                      ChatGPT 连接：已连接
```

底部可以保留：

- 其他配置方式；
- 停止 CodexLink 管理的运行环境；
- “CodexLink 只停止自己启动的进程”的简短 ownership 说明。

普通用户不在首页看到 Server URL、Runner enrollment、loopback port、Tunnel client path、内部 Job/Process 名称。

---

## 4. 项目页契约

项目页保持 RC5 的卡片式管理：

- 标题：`此电脑上的项目`；
- 明确的 `+ 添加项目`；
- 顶部显示已授权项目数量；
- 每个项目卡片显示项目名、路径、AI 访问状态；
- 允许移除项目授权；
- 移除授权不删除磁盘文件；
- 选择哪个目录就授权哪个 canonical exact root，不偷偷扩大到父目录。

UI 不把 CodexLink 内部 runtime 管理目录、Engine 工作目录或系统维护目录显示成普通用户项目。

当前项目页合同进一步固定为：

- `projects[]` 是用户授权路径集合，不存在“只能有一个当前工作区”的产品限制；
- 宽窗口默认三列卡片，随可用宽度响应式降为两列 / 一列；
- 卡片保留文件夹图标、名称、完整路径、`AI 访问 / 已就绪`、目录类型与“移除项目”；
- 移除只撤销 authority，不删除磁盘文件；父授权仍覆盖子目录时不得假装撤权成功。

---

## 5. 连接页契约

连接页保留 RC5 的“步骤链 + 当前状态”心智：

```text
ChatGPT 连接

连接 ChatGPT                      连接已建立
✓ 运行环境就绪
✓ Tunnel 凭据已配置
✓ 建立安全链路
✓ ChatGPT 可访问

当前状态
连接正常
运行环境   已就绪
Tunnel 凭据 已配置
连接方式   OpenAI Tunnel
当前网络路线 <仅在有意义时显示>
```

底部 Secure Tunnel 卡片只需要表达：

- 当前是否正常；
- 是否可用于 ChatGPT；
- 停止安全隧道。

用户不需要理解 tunnel-client 二进制、动态本地 health port 或 Server/Runner 启动参数。

---

## 6. 设置页契约

设置页首版重点只有三类内容：

1. 界面语言；
2. 随 Windows 登录自动启动；
3. OpenAI Tunnel 凭据。

Tunnel 凭据 UI 固定为两个用户需要保存的值：

```text
Tunnel ID
Restricted Runtime Key
```

实现要求：

- Tunnel ID 可以显示和重新编辑；
- Restricted Runtime Key 使用 Windows CurrentUser DPAPI 保存；
- Runtime Key 保存后不重新回显；
- 输入框留空表示保留已保存 key；
- 不把 Runtime Key 写进 `desktop-state.json`；
- 不把 key 打进日志、诊断或剪贴板；
- 保存后无需用户重启 CodexLink。

代理设置属于高级/排障能力。默认 `自动（推荐）`，只有网络异常或用户明确需要时才展开处理；不要让正常用户在首次配置里理解代理、loopback route 或端口。

---

## 7. 首次使用必须极简

用户已经通过 Deployment Center 验证购买并取得 installer 后，Desktop 内主流程冻结为“激活后自动 bootstrap，项目与连接独立配置”：

```text
安装 CodexLink
→ 用购买时的同一枚 MYWS 销售激活码激活设备
→ CodexLink 用 internal runtime anchor 自动准备本机 Engine / Runner
→ 如果还没有 Tunnel ID + Restricted Runtime Key，在“设置”保存一次
→ CodexLink 自动准备 / 恢复 Secure Link
→ 按需添加一个或多个本地授权路径
→ 用户去 ChatGPT 创建/添加一个 App
→ 连接完成
```

已有 Tunnel 凭据的老用户不应因为重装或 UI 迁移再次被要求手动“先启动 Runner、再重连 Tunnel”；没有凭据的全新用户则可以停在“待配置 Tunnel”状态，这是正常而且可解释的。

普通用户**不应该**再做：

- 手动启动 Server；
- 手动启动 Runner；
- 配 Server URL；
- 配 Runner enrollment；
- 选择本地端口；
- 找 tunnel-client；
- 手动设置 health URL；
- 为正常网络手动配置代理；
- 每次启动重新填 Tunnel key；
- 为正常开发命令反复点权限确认。

如果底层组件启动失败，CodexLink 应给“重试 / 查看诊断 / 网络设置”这种恢复动作，而不是把底层配置表单推给普通用户。

---

## 8. 保存 Tunnel 凭据后的行为

用户保存完整 Tunnel ID + Restricted Runtime Key 后，产品目标是：

1. 安全持久化凭据；
2. 若 internal runtime 尚未就绪，CodexLink 自动 bootstrap / 恢复，不要求用户先选业务项目；
3. Tunnel 凭据完整时，由 CodexLink 自动启动或重启所需 Secure Link；项目 authority 可以随后独立添加；
4. 连接页自动反映实际 readiness；
5. 首页最终收敛到“一切就绪”；
6. 用户下一步只需要按教程在 ChatGPT 创建/添加 App。

不要为了“可配置”给首次用户增加不必要分支。

---

## 9. 动效与视觉还原原则

由于当前没有恢复到 RC5 对应 TSX/CSS 源码，动效不凭截图臆测具体毫秒值。

实现时采用：

- 真实 RC5 程序逐页观察；
- 对照截图检查布局、间距、字体层级、卡片、阴影、状态点、按钮和滚动行为；
- 观察 hover / focus / page transition / ready-state transition；
- 只在当前 React/Tauri 中实现观察到的产品行为；
- 不为了“更炫”增加新的动画语言。

RC5 是视觉基准，不要求旧实现技术栈逐字节复刻。

---

## 9.5 2026-09-15 Smoke 2 历史恢复证据

当前 source checkpoint：`09c48060 — CodexLink: harden Tunnel routing and restore RC5 visuals`。

本轮不是把“差不多的图标/布局”当完成，而是继续按 RC5 成品反推：

- Windows EXE / installer / taskbar icon 与左上角 brand 已切回从真实 RC5 EXE 提取的资源；Smoke 2 新 EXE 再次抽出的 32px / 256px PNG 哈希与 RC5 原图完全一致；
- 首页项目卡恢复左上 availability、右侧双端点连接图与更接近 RC5 的链路节点；
- 项目页回到小卡片网格；连接页回到纵向步骤链；设置页补回 Tunnel route / Windows proxy 状态表达；
- Tunnel Auto 不再展示未经验证的第一候选代理为“有效路线”，只有实际通过 control-plane transport probe 的 route 才能成为有效值；
- TypeScript、51/51 Vitest、form-control CSS contract 与 production build 通过；Rust 串行完整套件 105/105 passed（5 ignored），focused Auto proxy 4/4；
- Smoke 2 unsigned installer SHA256：`35CBB972A8439D03184AD10BAB4E66714EA2C52E64D773DEB111A3EBDF3F9D71`。构建产物保留在本机 `target/`，不提交 Git、不覆盖历史 RC。

仍未宣称完成的部分：用户真机逐页视觉、高 DPI、hover/focus/滚动、真实 Tunnel 自动选路与 ChatGPT 访问。Smoke 2 的目的就是暴露这些剩余差异，不以本地单测替代产品验收。

---

## 9.6 2026-09-15 Smoke 3 / BrandFix 收口证据

Smoke 2 之后的多轮真机反馈已落实到当前未提交 worktree：

- internal runtime anchor 与 user authorized roots 分离；0 个项目不再阻塞 Runtime/Tunnel readiness；
- 多授权路径 + 精确 remove authority；项目页响应式 3/2/1 列；
- 首页只显示第一个授权路径，0 项目时给明确“添加项目”动作；
- `ChatGPT 可访问` 与“实际访问过一次”解耦；系统状态使用“已连接”；
- 连接链路视觉收紧，状态线更连续；
- RC5 风格渐变、动作列表、hover/page transition 已收口；
- Tray 只保留“打开 CodexLink / 退出 CodexLink”，并跟随 UI locale；
- 激活错误本地化，6 个用户语言包增加大小写不敏感 CodexLink 品牌守卫；
- 当前自动化证据：frontend `56/56`，Rust Desktop `114 passed / 0 failed / 5 ignored`，production build / fmt / diff check 通过；
- 最新 BrandFix installer SHA256：`B67AB5388095EAB2A256AA2E9EF94231A94E87FE8ED7EBCB939F0D9159C8FF71`。

仍待真实系统收口：Explorer 缓存后的最终图标确认、设备激活跨卸载再验、意外断线长期自愈、Remote Existing Server 同源迁移。

---

## 10. 首版 UI Gate

进入新 RC 前至少证明：

- 首次无有效 License 时只显示 Activation Gate，后台 runtime 不偷跑；
- 销售激活码页面保持 RC5 简洁 customer copy，并与 Deployment Center 使用同一商业词汇；

- 首页视觉心智恢复为 RC5 的“一切就绪 + 项目卡 + 连接链路”；
- 项目页恢复卡片式项目管理；
- 连接页恢复步骤链 + 当前状态；
- 设置页可保存 Tunnel ID + DPAPI Runtime Key；
- 保存凭据后无需用户手动启动 Server/Runner/Tunnel client；
- 默认本机主流程中不出现 Server URL、Runner enrollment、端口等高级概念；Existing Server 只放在“其他配置方式”高级入口；
- 当前项目、Tunnel、ChatGPT readiness 来自真实状态，不是假绿灯；
- 中英文与至少现有受支持语言不因 UI 恢复而破坏；
- 键盘 focus、滚动、窗口最小尺寸和高 DPI 可用；
- 新 UI 不削弱 `CODEXLINK_SECURITY_BOUNDARY.md` 中的安全 Gate。

---

## 11. 首发以后

首发后可以根据真实用户反馈微调信息密度、引导和视觉细节，但默认继续沿用这套 RC 产品心智。

只有在用户明确决定重新设计产品信息架构时，才开启新的 UI V2 项目；UI V2 不再作为首版默认后续任务自动推进。
