# CodexLink License Operator

> 状态：**2026-09-15 Windows 本地管理员签发工具 v1 已实现、用户真实一键签发验收通过，并已作为 `4b4043ca — CodexLink: add local license operator` 提交/推送。**
>
> 定位：这是 CodexLink 销售/运营侧的本地管理员工具，不是客户软件，也不是公开授权 API。

## 1. 目标

CodexLink License Operator 用于在管理员自己的 Windows 电脑上一键签发新的 `MYWS-...` 销售激活码：

```text
管理员 Windows
  → 本地 GUI
  → Windows OpenSSH + 管理员自己的 SSH key
  → 授权 VPS
  → /usr/local/bin/myws-license-admin issue --json
  → 返回一枚新的 MYWS 销售激活码
  → GUI 显示并复制到剪贴板
```

授权服务的签名私钥始终留在服务器。Operator 不下载、复制或读取服务器签名私钥，也不新增公网管理 API。

## 2. 当前服务端合同

当前授权服务器已经存在管理员入口：

```text
/usr/local/bin/myws-license-admin issue --json
```

实际 Python CLI entry point：

```text
myws_license.cli:main
```

已核实 `issue --json` 返回单行 JSON：

```json
{"code":"MYWS-XXXXX-XXXXX-XXXXX-XXXXX","activation_code_id":"<id>"}
```

数据库仍以 `/opt/myws-license/state/licenses.db` 作为 SQLite state；签发后记录为未使用状态，数据库保存销售码哈希而不是销售码明文。

当前管理员能力已知有 `issue`、`public-key`、`verify-license`、`init`；当前没有销售码专用 `revoke`、`list`、`inspect`。因此 Operator v1 **只做 issue**，不在客户端伪造不存在的管理能力。

> v1.1.0 更新：服务端已增加 `myws-license-admin` 运营命令（查码、停用 / 恢复、批量签发、批次统计、操作记录、备份），由商业仓库的本机运营工作台调用。本 Operator 不变，仍只做 issue。

## 3. 非幂等安全规则

`issue` 是非幂等操作：每次调用都会创建一枚新的销售激活码。

因此 Operator 必须遵守：

- 单次点击只发起一次远端 `issue --json`；
- 网络失败、SSH exit 异常或超时后**绝不自动重试**；
- 只要 stdout 已经出现一枚格式有效的 `MYWS-...` JSON，就把这枚码视为本次签发结果；
- 即使 SSH 会话收尾异常，也不能因为“没有拿到理想 exit code”再次签发；
- 用户只有在明确判断本次没有产生可用销售码后，才能再次手动点击生成。

这条规则优先于普通“失败自动重试”体验，目的是避免一次点击意外创建多枚销售码。

## 4. 本地安全边界

Operator 配置只保存：

- SSH host；
- SSH port；
- SSH user；
- SSH private-key **path**。

Operator 不保存：

- SSH 私钥内容；
- 授权服务器签名私钥；
- 已生成的销售激活码；
- `activation_code_id` 历史；
- 客户设备 License；
- Tunnel / WebCodex credential。

生成成功后销售码只显示在当前窗口并复制到 Windows 剪贴板。关闭工具后不提供本地历史列表。

SSH 使用 `BatchMode=yes`、public-key only、`StrictHostKeyChecking=yes`，不在 GUI 中接受密码或自动信任新的主机指纹。

## 5. Windows 实现

仓库源码：

```text
scripts/codexlink_license_operator.ps1
scripts/install_codexlink_license_operator.ps1
```

PowerShell 源码保存为 UTF-8 BOM，以兼容 Windows PowerShell 5.1 在中文 Windows 上的 `-File` 解码行为。

默认本机安装位置：

```text
%LOCALAPPDATA%\CodexLink\LicenseOperator\
  config.json
  app\
    CodexLink-License-Operator.ps1
    CodexLink.ico
```

安装脚本可以创建桌面快捷方式。快捷方式直接启动 Windows PowerShell STA GUI，不需要用户打开终端。

Operator 会显式 redirect/close `ssh.exe` stdin。这样 GUI 进程即使没有 Console stdin，Windows OpenSSH 也能正常得到 EOF 并返回 exit code。

## 6. 错误处理

成功判断的权威来源是 stdout 中唯一且格式有效的销售码 JSON。

stderr 用于故障说明，但不能覆盖已经收到的有效销售码。当前 VPS 存在一条非阻塞 sudoers duplicate-alias warning；Operator 会过滤这类已知噪声，不把它误判成签发失败。

如果没有收到有效销售码，Operator 才根据 SSH exit / timeout / stderr 给出失败状态。失败不会触发后台重试。

## 7. 当前验证

2026-09-15 已完成：

- Windows PowerShell 5.1 source self-test；
- 本地安装副本 self-test；
- 桌面快捷方式 target / icon / config 路径核验；
- 使用安装副本执行真实 SSH `PROBE_OK`，未签发销售码；
- 另用同一 SSH/CLI 合同真实签发 1 枚 unused 集成测试销售码，以确认 `issue --json` schema；该销售码没有写入仓库、Operator 配置或本地工具文件。
- 用户已实际打开安装后的 GUI 完成一次一键签发并确认体验可用；该人工验收是 Batch H.0 checkpoint 的完成条件之一。

## 8. 后续管理能力

未来如果确实需要销售运营后台，再单独设计服务端管理能力，例如：

- list / inspect；
- revoke / disable（v1.1.0 已由运营工作台实现，见第 2 节）；
- 销售订单备注或外部订单号；
- 设备绑定状态查询；
- 换机 / 解绑支持流程。

这些能力必须先在 License Core 定义明确的服务器端合同，再由 Operator 调用；不要让本地工具直接读写 `licenses.db`，也不要把签名私钥搬到本机。
