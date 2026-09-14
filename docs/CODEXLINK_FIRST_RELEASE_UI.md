# CodexLink First-Release Product Shell & Setup UX

> 状态：**2026-09-14 正式敲定，首个商业版本的 UI / UX 基线。**
>
> 核心决策：**首版沿用已验收 RC5 成品的产品壳、信息层级、状态表达和交互风格，不再为首发另起一套 Dashboard 视觉方向。**
>
> 实施状态：**Batch G 已完成本地 Product Shell / 首用流程恢复；真实新 RC 与历史 RC5 的逐页视觉、动效和高 DPI 对照仍属于真机发布验收。**
>
> 目标：用户只理解“项目”和“ChatGPT 连接”，Server、Runner、端口、进程、Tunnel client 等实现细节由 CodexLink 自动处理。

本文是首版 CodexLink 产品壳与首次配置体验的权威规范。安全边界仍以 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md) 为准；购买资格与设备激活合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

---

## 1. 已冻结的产品方向

首版 UI 以历史 `CodexLink 1.0.0-rc.5` 成品为视觉和交互基准。

保留其核心心智：

- 左侧固定产品导航：`首页 / 项目 / 连接 / 设置`；
- 顶部显示当前工作区与全局就绪状态；
- 首页以“一切就绪”或明确的下一步为主，而不是展示底层 runtime 术语；
- 当前项目使用大卡片表达；
- `本机 → Engine → Secure Link → ChatGPT` 使用清晰的连接链路表达；
- “接下来做什么”与“系统状态”只给用户需要的动作和状态；
- 项目页使用卡片式项目管理；
- 连接页使用步骤链说明运行环境、Tunnel 凭据、安全链路与 ChatGPT 是否已验证；
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
- 激活成功后才进入项目选择 / Product Shell；
- 客户页面不显示签名算法、DPAPI、Device ID、内部授权 endpoint 等实现细节；
- 激活失败只给客户可理解的重试 / 支持动作，不提供绕过 Activation Gate 的高级开关。

详细商业合同见 [`CODEXLINK_COMMERCIAL_FLOW.md`](CODEXLINK_COMMERCIAL_FLOW.md)。

---

## 3. 首页契约

首页回答三个问题：

1. 现在能不能用；
2. 当前授权的是哪个项目；
3. 本机到 ChatGPT 的链路是否正常。

目标结构：

```text
当前工作区                  已就绪

首页
一切就绪
本机项目已经准备好，可以继续交给 ChatGPT 工作。

[当前项目大卡片]
项目名 / 路径 / AI 连接 / 运行环境 / 项目数量
查看项目 | 连接详情

[连接链路]
本机 —— Engine —— Secure Link —— ChatGPT

[接下来做什么]        [系统状态]
查看项目              CodexLink 服务
管理连接              Runner
                      项目
                      ChatGPT 连接
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

---

## 5. 连接页契约

连接页保留 RC5 的“步骤链 + 当前状态”心智：

```text
ChatGPT 连接

连接 ChatGPT                      连接已建立
✓ 运行环境就绪
✓ Tunnel 凭据已配置
✓ 建立安全链路
✓ ChatGPT 已访问

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

用户主流程目标冻结为：

用户已经通过 Deployment Center 验证购买并取得 installer 后，Desktop 内主流程固定为：

```text
安装 CodexLink
→ 用购买时的同一枚 MYWS 销售激活码激活设备
→ 选择项目
→ 在“设置”填 Tunnel ID + Restricted Runtime Key
→ 保存
→ CodexLink 自动准备 Engine / Runner / Secure Link
→ 用户去 ChatGPT 创建/添加一个 App
→ 连接完成
```

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
2. 若项目/运行环境尚未就绪，提示用户只完成缺失的“选择项目”动作；
3. 若 runtime 已就绪，则由 CodexLink 自动启动或重启所需 Secure Link；
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

## 10. 首版 UI Gate

进入新 RC 前至少证明：

- 首次无有效 License 时只显示 Activation Gate，后台 runtime 不偷跑；
- 销售激活码页面保持 RC5 简洁 customer copy，并与 Deployment Center 使用同一商业词汇；

- 首页视觉心智恢复为 RC5 的“一切就绪 + 项目卡 + 连接链路”；
- 项目页恢复卡片式项目管理；
- 连接页恢复步骤链 + 当前状态；
- 设置页可保存 Tunnel ID + DPAPI Runtime Key；
- 保存凭据后无需用户手动启动 Server/Runner/Tunnel client；
- 正常用户主流程中不出现 Server URL、Runner enrollment、端口等高级概念；
- 当前项目、Tunnel、ChatGPT readiness 来自真实状态，不是假绿灯；
- 中英文与至少现有受支持语言不因 UI 恢复而破坏；
- 键盘 focus、滚动、窗口最小尺寸和高 DPI 可用；
- 新 UI 不削弱 `CODEXLINK_SECURITY_BOUNDARY.md` 中的安全 Gate。

---

## 11. 首发以后

首发后可以根据真实用户反馈微调信息密度、引导和视觉细节，但默认继续沿用这套 RC 产品心智。

只有在用户明确决定重新设计产品信息架构时，才开启新的 UI V2 项目；UI V2 不再作为首版默认后续任务自动推进。
