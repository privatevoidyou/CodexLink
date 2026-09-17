# CodexLink First-Release Commercial Entitlement & Activation Flow

> 状态：**2026-09-17 v1 首发商业主链进一步收敛为：设备激活 + 两步首次引导 + Local Engine/Runner + OpenAI Tunnel + ChatGPT + 本地授权路径。Existing Server、Quick Share 与“其他配置方式”不进入首版客户 UI/教程；完整 Deployment Center 仍不作为第一版桌面发布的硬阻塞项。**
>
> 核心决策：**同一枚 `MYWS-...` 销售激活码既是购买资格凭证，也是 CodexLink 首次设备激活凭证；它不是单纯的下载密码。**
>
> 目标：把“购买 → 下载 → 本机激活 → Runtime 自动准备 → Tunnel 凭据 → 按需授权路径 → ChatGPT App”做成一条真实、简单、可测试的商业主链。

本文是 CodexLink 首版商业 entitlement / activation 的权威规范。产品 UI 见 [`CODEXLINK_FIRST_RELEASE_UI.md`](CODEXLINK_FIRST_RELEASE_UI.md)，安全边界见 [`CODEXLINK_SECURITY_BOUNDARY.md`](CODEXLINK_SECURITY_BOUNDARY.md)。

---

## 1. 商业原则

CodexLink 卖的是**产品使用授权**，不是一个可复制的安装包链接。

因此：

- 用户购买后获得一枚 `MYWS-XXXXX-XXXXX-XXXXX-XXXXX` 销售激活码；
- 网站使用这枚码验证购买资格并展示下载 / 教程入口；
- 安装后的 CodexLink 使用**同一枚码**完成设备激活；
- 没有有效本机 License 时，不允许启动可工作的 Server / Runner / Tunnel runtime；
- 已激活设备以后通过本地签名 License 验证，不要求每次启动都在线联系授权服务；普通卸载/重装也不应被当成“换了一台设备”；
- 安装包即使被单独复制，也不等于获得 CodexLink 使用授权。

首版不把购买资格、下载资格和设备激活拆成三套不同凭证。

---

## 2. 标准客户主链

首批商业发布允许两种下载入口：Deployment Center（若已完成）或人工交付的百度网盘 / 夸克网盘下载链接 + 图文教程。无论安装包从哪里取得，Desktop 客户主链保持一致：

```text
购买 CodexLink
→ 获得 MYWS 销售激活码 + 安装包/下载入口 + 图文教程
→ 安装 CodexLink
→ 首次启动输入 MYWS 激活码
→ 完成设备绑定并取得 Signed License
→ CodexLink 使用 internal runtime anchor 自动准备 Engine / Runner
→ 首次引导 1：创建/填写 Tunnel ID + Restricted Runtime Key（已有则跳过）
→ 首次引导 2：在 ChatGPT 创建/添加 CodexLink App（已有则跳过）
→ 进入主页面
→ 按需添加一个或多个本地授权路径
→ 开始使用
```

若 Deployment Center 已上线，同一枚 `MYWS-...` 仍可先用于网站验证购买资格，再用于 Desktop 设备激活；如果第一批采用人工分发，则网站验证步骤不是 Desktop v1 的硬依赖。所有渠道必须使用同一套激活、onboarding 与安全说明。

---

## 3. 两个阶段必须明确区分

### 3.1 网站：验证购买资格

Deployment Center 的激活码输入承担的是 entitlement / download gate：

- 验证用户是否持有有效购买凭证；
- 验证通过后显示当前推荐安装包、版本信息、SHA256、教程和支持入口；
- 明确提示“此激活码稍后还要用于 CodexLink 首次设备激活，请妥善保存”；
- 网站验证本身不应伪装成“这台电脑已经激活”；
- 网站验证购买资格不应因为用户查看一次下载页就自动完成设备绑定。

推荐客户文案优先使用：

- `验证购买`
- `销售激活码`
- `验证并查看下载`

而不是让用户误以为这是一次性的短信“验证码”。

### 3.2 Desktop：设备激活

CodexLink 首次启动时的 Activation Gate 才决定这台设备能否进入产品工作状态。

用户输入同一枚 `MYWS-...` 销售激活码后：

1. Desktop 生成 / 读取当前设备身份；
2. 调用授权服务完成设备激活；
3. 获得并验证 Signed License；
4. 将 License 与受保护的设备身份持久化到独立设备资格目录 `%LOCALAPPDATA%\CodexLinkSecretKey\`；
5. 激活成功后才进入 RC5 Product Shell 和 runtime setup。

销售激活码本身不长期写入 Desktop state。

设备资格与普通 app/runtime data 分离：`%LOCALAPPDATA%\dev.codexlink.desktop` 可以承载可重建的运行状态，而 `CodexLinkSecretKey\device_identity.dpapi + license.json` 不应因为卸载器清理普通应用数据而被删除。当前实现还兼容从 `CodexLink.DeviceActivation.v1` 与更旧 app-data `license` 目录迁移。

---

## 4. 本机 Activation Gate

未激活或 License 无法验证时：

```text
CodexLink Desktop
→ Activation Gate
→ 不启动 Server
→ 不启动 Runner
→ 不启动 Tunnel
→ 不创建可工作的项目 runtime
```

允许的只是完成激活、显示客户可理解的恢复信息以及必要的产品/支持入口。

激活页保持简洁：

```text
设备激活
激活 CodexLink
首次使用时，请完成设备激活。

销售激活码
[ MYWS-XXXXX-XXXXX-XXXXX-XXXXX ]

[ 激活 CodexLink ]
```

普通客户 UI 不显示 Ed25519、签名 payload、DPAPI、Device ID、内部 endpoint 或私钥实现。

---

## 5. 已完成的客户端激活合同

当前 rebuild 已恢复并验证：

- RC5 SignedLicense 合同；
- CurrentUser DPAPI 设备身份；
- `device_id` 派生；
- 客户端只包含公钥验证材料，不包含私有签名 key；
- 未激活时 runtime-creating operation fail closed；
- 已激活设备可离线验证 License；
- 历史 RC5 `license.json + device_identity.dpapi` 与当前 LicenseManager 兼容。
- 当前工作树已把设备激活目标目录迁到 `%LOCALAPPDATA%\CodexLinkSecretKey\`，并增加跨旧目录迁移与“删除普通 app data 后设备资格仍存在”的自动化回归；
- Activation error 由 `error.code` 映射到当前 UI locale，后端英文 `next_action` 不再直接暴露给中文用户；
- Signed License 内部 `product: "webcodex-deployment"` 暂时作为签名协议字段保留，不能客户端单边改名；未来如迁移必须同时升级服务端签发端和客户端验证端。

实现 checkpoint：`733b7b54 — CodexLink: restore RC5 device activation`。

---

## 6. 首批分发与 Deployment Center

`deploy.syzdq.xyz/#activate` 方向继续保留，但**完整 Deployment Center 不再阻塞第一批商业发布**。v1 可以先使用百度网盘 / 夸克网盘等人工管理渠道交付 installer，并随下载资料提供版本、SHA256、激活方式和完整图文 Quick Start。

首批分发至少要让用户拿到：

- 当前推荐 CodexLink Windows installer；
- Product version、发布日期、SHA256、签名状态（若 unsigned 必须明确）；
- `MYWS-...` 销售激活码及“用于 Desktop 首次激活”的说明；
- 两步首次引导对应的图文教程：创建 Tunnel / Restricted Runtime Key，以及在 ChatGPT 创建/添加 CodexLink App；
- 常见失败恢复与支持联系方式；
- 简明安全边界说明。

Deployment Center 完成后可以把上述人工流程自动化为“验证购买 → 查看下载/教程”，但不应为了首版引入账号系统、自动 updater、CDN 或复杂下载 token 体系。

### 6.5 管理员销售码签发工具

销售码签发属于管理员侧能力，不进入客户 Desktop，也不新增公网管理 API。当前 v1 采用本地 License Operator：

```text
管理员 Windows GUI
→ 使用管理员自己的 SSH key 登录授权 VPS
→ 固定调用 /usr/local/bin/myws-license-admin issue --json
→ 返回一枚新的 MYWS 销售激活码
→ 显示并复制到剪贴板
```

关键边界：

- 服务器签名私钥永不离开授权服务器；
- 本地只保存 SSH host / user / port / private-key path，不复制私钥内容；
- 销售码不写入 Operator 配置或本地历史；
- `issue` 是非幂等操作，Operator **绝不自动重试签发请求**；
- stdout 已收到有效 `MYWS-...` 时，即使 SSH 收尾异常也必须认定本次已经签发，不能再次自动 issue；
- 当前服务端没有销售码专用 `revoke` / `list` / `inspect`，v1 不伪造这些能力。

实现与运维合同见 [`CODEXLINK_LICENSE_OPERATOR.md`](CODEXLINK_LICENSE_OPERATOR.md)。

---

## 7. 极简配置与商业激活的关系

“配置越简单越好”**不等于删除商业激活**。

普通用户首用只需要理解四类输入：

1. 购买后收到的销售激活码（首次设备激活）；
2. OpenAI Tunnel ID；
3. Restricted Runtime Key；
4. 需要让 ChatGPT 访问时，再添加一个或多个本地授权路径。

Server、Runner、端口、enrollment、tunnel-client path、health URL 等全部属于实现细节，默认自动处理和隐藏。v1 客户界面也不提供 `连接现有 Server`、Quick Share 或 topology chooser；这些底层能力保留为 hidden experimental/Post-v1，不写入首批销售教程，也不构成商业验收 Gate。

所以最终用户心智是：

```text
购买 / 激活
→ CodexLink 自动准备内部 Runtime
→ 两项 OpenAI Tunnel 凭据（若尚未保存）
→ 按需添加本地授权路径
→ ChatGPT App
```

---

## 8. 恢复与失败路径

### 网站验证失败

- 显示“激活码无效 / 无法验证购买资格”这类 customer-level 信息；
- 不暴露授权数据库或签名实现；
- 网络故障与无效码要区分，避免用户误以为购买失效。

### Desktop 激活失败

- 激活服务不可达：允许重试，不启动 runtime；
- 激活码无效：明确提示检查购买码或联系支持；
- 本地 License 损坏 / 无法验证：进入 recovery activation，而不是绕过 Gate；
- 已有有效 License：正常启动不依赖网站在线。
- 卸载/重装：只要独立设备资格目录仍在且 License 有效，应直接识别同一设备；不得因为删除普通 app data 就要求重新消费销售激活码；
- 已绑定到另一设备：按当前商业策略进入支持流程，不在客户端提供绕过绑定的隐藏开关。

设备数量、换机、解绑、退款等商业政策由授权服务 / 支持政策单独定义；客户端首版不自行发明绕过机制。

---

## 9. 安全与隐私边界

- 私有签名 key 只存在于受控服务端环境，不进入安装包、网站前端或客户端日志；
- 销售激活码不写入普通 Desktop state，不在日志和 diagnostics 回显；
- Signed License 可持久化，本地只做验证；
- 网站和 Desktop 都只显示客户需要的状态，不泄露内部授权实现；
- 下载页验证成功不等于设备激活成功；
- Activation Gate 不得被 UI shortcut、autostart 或 background resume 绕过。
- Remote enrollment 的长期 user token / token-bearing Runner config 属于 bearer credential，Windows 上必须使用 protected DACL secret-file primitive 持久化；这与销售激活码/设备 License 是不同的凭据域。

---

## 10. 首版商业验收 Gate

在 Stable 之前必须实际证明：

1. 用户通过当前首批分发渠道可以取得正确 installer、版本、SHA256、激活码和图文教程；
2. 新机器安装后，未激活时 Server / Runner / Tunnel 不偷跑；
3. 有效销售激活码可以完成 Desktop 设备激活；无效码给出明确 customer-level 错误；
4. 激活成功后重启可离线验证，不要求每次联网；
5. 激活码不出现在普通 state、日志、activity、diagnostics；
6. 激活成功后首次用户进入两步 onboarding；已有配置用户可跳过，跳过不伪造 readiness；
7. upgrade / uninstall / reinstall 不删除 `%LOCALAPPDATA%\CodexLinkSecretKey\` 设备资格；即使普通 app data 被清理，同设备仍能恢复已激活状态；
8. clean-machine 外部测试者可以不理解 Server/Runner/端口等底层概念完成“安装 → 激活 → 两步引导 → 主页面 → 授权项目 → ChatGPT 使用”；
9. v1 正式界面不出现 Existing Server、Quick Share、模式选择页或“其他配置方式”；首发支持和教程只围绕 Local + OpenAI Tunnel 主链；
10. 人工分发资料、Desktop 和未来 Deployment Center 使用一致的产品词汇和同一条客户主链；
11. 若 Deployment Center 在首发时已经可用，再额外验证购买资格 Gate 与 invalid-code 下载保护；否则这两项留给网站上线验收，不阻塞 Desktop v1。

---

## 11. 非目标

首版不因为商业闭环而引入：

- 强制登录账号系统；
- 自动更新；
- installer 内购买流程；
- 多层下载 token 体系；
- 每次启动在线 License 心跳；
- 在 v1 客户表面产品化 Existing Server / Remote enrollment 或 Quick Share；
- 把加密 / 签名实现细节暴露给普通用户。

商业闭环要真实，但实现仍然保持小而清楚。
