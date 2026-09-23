# CodexLink First-Release Security Boundary

> 状态：**首版安全边界规范。具体源码与发布验收状态见 [`CODEXLINK_V1_ACCEPTANCE.md`](CODEXLINK_V1_ACCEPTANCE.md)；历史测试结果不能替代最终安装包的真机验收。**
>
> 适用范围：CodexLink Desktop、CodexLink 管理的 WebCodex Server / Runner、模型可见的 typed file tools 与本地 execution admission。
>
> 核心决策：**首版不做 OS sandbox；允许正常开发 execution；重点防止 Agent 常见的大范围误删、误覆盖和明显 destructive shell。**

本文是 CodexLink 首版安全策略的详细单一来源。当前验收文档只保留摘要并引用本文，避免安全规则在多份文档中漂移。

历史 dogfood runtime identity `WebCodex 0.4.1 / ae858f5360ff / dirty=false` 仅供追溯。真实安全验收必须针对当前 bundled fork build 与最终安装包。

---

## 1. 威胁模型与非目标

### 1.1 首版要防什么

CodexLink 首版重点降低以下高概率事故：

- Agent 因 cwd 判断错误删除了错误目录；
- 对 project root、父目录、磁盘根或关键系统目录做 destructive operation；
- `rm -rf`、`Remove-Item -Recurse`、`rmdir /s` 等宽泛递归删除；
- wildcard / broad delete 导致范围远大于模型预期；
- `git clean -fdx`、`git reset --hard`、`git restore .` 等大范围 Git 破坏；
- `robocopy /MIR` / `/PURGE` 等镜像或清空行为；
- `format`、`diskpart clean`、`Clear-Disk` 等磁盘级破坏；
- symlink / junction / reparse point 把一个看似安全的项目内目录导向项目外位置；
- create / rename / edit 在并发变化后意外覆盖用户的新内容；
- CodexLink 自身 installer / migration / staging 使用无边界 broad recursive cleanup。

### 1.2 首版明确不解决什么

CodexLink **不声称**提供恶意代码 containment 或完整 OS 隔离。

例如：

```text
python malicious.py
node malicious.js
```

脚本本身如果故意调用系统 API 删除当前 Windows 用户本来就有权限访问的文件，首版 Destructive Guard 不保证能够阻止。

因此产品文案不得声称：

- “AI 只能访问项目目录”意味着 Windows token 已被 OS 隔离；
- “允许一个项目”意味着恶意进程无法访问其他路径；
- CodexLink 可以安全运行不可信恶意代码；
- command blacklist 等价于 sandbox。

如果未来要解决恶意代码 containment，应作为独立的可选强隔离项目重新评估，不阻塞首版。

---

## 2. 分层安全模型

首版采用三层半模型：

```text
ChatGPT / Agent
    ↓
短而稳定的 safety instructions
    ↓
Typed file contracts + Destructive admission
    ↓
WebCodex Runner / Windows filesystem
    ↓
CodexLink-owned process lifecycle / DPAPI secrets / installer ownership
```

### 2.1 第一层：模型侧安全指引

CodexLink 向模型提供简短、稳定、不啰嗦的规则：

- 删除、清理、覆盖前检查 cwd、目标路径、递归参数和 wildcard；
- 优先使用 CodexLink typed file tools；
- 大范围目录删除使用受保护的 `delete_directory`，不要用 raw recursive shell delete；
- 不对授权根、父目录、磁盘根、关键系统 anchor 做 destructive operation；
- 无法确定 destructive scope 时停止，而不是猜。

这一层只降低模型犯错概率，**不是安全边界**。

### 2.2 第二层：typed 文件契约

能用结构化文件操作表达的增删改读，优先走 typed API，而不是让模型拼 shell。

### 2.3 第三层：Destructive Guard

明显 destructive 的 raw shell / process 请求在 spawn 前 fail closed。

Guard 的目标不是分析所有程序语义，也不是自己实现 PowerShell / bash / cmd 解释器；它只覆盖高概率、毁灭性、可低成本可靠识别的常见事故模式。本轮额外拒绝 PowerShell `-EncodedCommand/-enc/-ec/-e` 这类直接隐藏 payload 的入口，但不会继续扩张成 Python/Node/脚本内部语义分析器。

**关键 capability contract：**

- typed file/Git/project operations 是 CodexLink-managed 强权限边界；
- raw shell / arbitrary process / script / PowerShell / Python / Node 属于当前 Runner OS 用户权限下的高级 arbitrary-execution capability；
- `allowed_roots` 与 launch cwd 不等于 OS sandbox，child process 仍拥有该 Windows 用户本来的系统访问能力；
- Destructive Guard 是 mitigation / 最后一层误操作保护，不是 arbitrary code containment。

### 2.4 底层已有边界

首版继续依赖并保持：

- exact project authority / canonical path；
- internal runtime anchor 与 user authorized roots 分离；internal anchor 只维持 CodexLink Runtime identity，不自动成为用户项目 authority；
- guarded edit / expected SHA；
- License 与 Tunnel secret 的 DPAPI protection；
- Windows V1 安装包中的三颗 CodexLink Core 使用独立 compile-time bundled product mode：普通 operational 入口只接受安装目录内 `CodexLink.exe -> CodexLink Core`（以及已进入该产品进程链后的 Core 子进程）启动；`--version` 保留诊断能力。通用 WebCodex Engine build 不启用这条 gate，因此 Remote Server / 通用 Engine 架构保持不变。该边界用于关闭“安装包内 Core 直接当第二套产品入口运行”的路径，不宣称提供同一 Windows 用户对二进制篡改/伪造父进程的 cryptographic anti-tamper。
- 长期 Runner enrollment bearer credential 使用统一 Windows protected secret-file primitive：user token 与 token-bearing `runner.toml` 禁用继承，只授予当前 Windows 用户与 SYSTEM FullAccess；ACL/reparse hardening 失败时 fail closed，overwrite/migration 后不得退化；
- CodexLink-owned process tree / Job Object 生命周期；
- installer / migration / staging 的明确 ownership；
- 日志和 diagnostics 的 credential 最小化。

---

## 3. Typed 增删改读契约

### 3.1 Read

`read` / `search` 应继续遵守：

- 目标在已授权 root 内；
- canonicalize 后不得越界；
- traversal / symlink / junction 不得扩大授权；
- 大文件使用 bounded range，不把无界内容一次灌给模型。

### 3.2 Create

`create_file(path, content)`：

- 目标必须在授权范围内；
- 目标默认必须不存在；
- **create 不隐式变成 overwrite**；
- 已存在时 fail closed，模型应改用 edit。

### 3.3 Edit

`edit_file` / patch：

- 优先 patch / bounded edit，而不是无理由整文件重写；
- 使用 `expected_sha` / 当前内容版本作为并发保护；
- 文件在模型读取后被 IDE、Git、用户或另一个 Agent 修改时，返回 conflict 并要求重新读取；
- 不因“最后写入者获胜”覆盖用户刚完成的修改。

### 3.4 Rename / Move

`rename_file(old, new, expected_sha)`：

- source 和 destination 都必须在授权范围内；
- source 必须仍匹配预期版本；
- destination 默认不得已存在；
- rename 不隐式覆盖另一个文件。

### 3.5 Delete file

`delete_file(path, expected_sha)`：

- 只接受一个明确普通文件；
- 目标必须在授权范围内；
- 不接受 wildcard；
- 文件若已被用户修改，SHA 不一致则拒绝；
- 不能把 directory 当 file delete 处理。

---

## 4. Bounded Recursive Delete

### 4.1 核心原则

首版**允许递归删除目录**，但只允许：

> **一个可以被 CodexLink 精确证明边界的明确目录子树。**

正式 typed capability：

```text
delete_directory(path)
```

它的语义就是“递归删除这一个明确目录树”，而不是“执行一条任意 recursive shell”。

因此下面是正常场景：

```text
delete_directory("node_modules")
delete_directory("target")
delete_directory("dist")
delete_directory("src/legacy")
```

目录很深、文件很多或体积很大，本身**不是拒绝理由**。

### 4.2 轻量 preflight

删除前只做 O(1) / 小常数成本检查，不完整预扫描目录树：

1. 解析并 canonicalize target；
2. target 必须是一个明确路径；
3. target 必须位于当前授权范围内；
4. target 不能等于授权 root 本身；
5. target 不能是授权 root 的父目录、filesystem root 或 drive root；
6. target 不得包含 wildcard / glob 语义；
7. 顶层 target 不是 symlink / junction / reparse point；
8. target 不得命中受保护 anchor。

禁止为了“安全摘要”先完整扫描几十万文件再走第二遍删除。

### 4.3 单遍递归

删除实现应：

```text
preflight target
    ↓
walk tree once
    ↓
每个 entry 检查 reparse / boundary
    ↓
立即处理当前 entry
    ↓
顺便累计统计
```

安全检查不得要求第二次完整遍历目录树。

### 4.4 Reparse / junction / symlink

递归删除**绝不 follow** reparse point。

第一版规则：

- 顶层 target 是 reparse/junction/symlink → fail closed；
- 递归过程中遇到 reparse/junction/symlink → fail closed，不进入其目标；
- 不因为链接表面路径位于项目内，就访问链接指向的项目外位置。

未来如需“只删除链接本身”的语义，可作为单独能力设计；首版不混在 recursive delete 中。

### 4.5 删除结果摘要

删除过程中顺便累计：

- deleted files；
- deleted directories；
- 可安全取得时的 deleted bytes。

完成后只返回紧凑摘要，例如：

```text
已删除 node_modules
38,241 files · 4,812 directories · 2.6 GB
```

不把数万条文件名刷给用户或模型。

---

## 5. 永久保护的 destructive anchors

即使授权范围很宽，首版也要保留少数不可由 Agent 自动 destructive-delete 的 anchor。

至少包括：

- filesystem / drive root：`C:\`、`D:\`、`/`；
- 当前授权 root 本身；
- 授权 root 的父目录；
- Windows 系统核心 anchor（如 Windows / Program Files / Users 根）；
- 当前用户 profile 根本身；
- CodexLink app state / runtime / connection 根；
- `%LOCALAPPDATA%\CodexLinkSecretKey` 设备激活根及其 `device_identity.dpapi` / `license.json` 敏感子树；
- repository metadata anchor（至少 `.git` 根目录）。

注意：保护 anchor 不等于永远禁止清理其所有安全子目录。以后若产品明确支持系统清理，应逐项定义可删除的具体子树，而不是允许删除 anchor 本身。

如果用户显式授权 `C:\`：

- 可以分析 / 列出 C 盘；
- `delete_directory("C:\")` 永远拒绝；
- 只有明确、被授权且不命中 protected anchor 的具体子目录才可能进入 bounded delete。

---

## 6. Raw Shell / Process Execution

### 6.1 产品目标与 capability 边界

CodexLink 首版允许正常开发 execution，不因为“用了 shell / Python / Node / PowerShell”就频繁询问用户。这里必须把两类 authority 明确分开：

- **Typed file / Git / project operations**：继续受用户明确添加的 exact project roots、canonical/reparse 检查和对应 typed contract 约束；
- **Raw shell / process execution**：表示允许 Agent 以当前 Runner OS account / 当前 Windows 用户权限启动开发命令。`cwd` 与 authorized roots 用于选择项目上下文和 typed authority，**不构成 child process 的 filesystem sandbox**。

能表达成 typed operation 的文件/Git mutation 继续优先使用 typed contract。产品 UI 不得再把“只授权项目路径”写成对 raw process 的 OS 级文件系统隔离承诺。

正常示例：

```text
cargo check
cargo test
npm test
npm run build
python build.py
node scripts/build.js
powershell ./scripts/package.ps1
git status
git diff
git log
```

### 6.2 明显 destructive shell 在 spawn 前拒绝

第一版重点识别并拒绝这些类别：

- recursive delete：`rm -rf` / `rm -r`、`Remove-Item -Recurse`、`rmdir /s` / `rd /s`；
- broad / wildcard delete；
- project / authorization root、parent、drive/filesystem root destructive target；
- destructive Git：`git clean -fd/-fdx`、`git reset --hard`、大范围 `git restore` / `git checkout --`；
- mirror / purge：`robocopy /MIR`、`robocopy /PURGE`；
- disk / partition destructive primitives：`format`、`diskpart clean`、`Clear-Disk` 等。

对 raw recursive delete 的标准反馈是：

> CodexLink 阻止了未受控的递归删除。请使用受保护的 `delete_directory`，或由用户在本机 Terminal 手动完成该操作。

### 6.3 不实现“危险命令百科全书”

不要维护几百条字符串 blacklist，也不要尝试完整解析 PowerShell / bash / cmd。

Guard 只维护少量 destructive classes：

```text
recursive_delete
broad_delete
root_or_parent_delete
destructive_git
mirror_or_purge
disk_destructive
opaque_encoded_command
```

每个 shell dialect 只识别少数高置信 primitive。

### 6.4 明确承认绕过边界

例如：

```text
python -c "import shutil; shutil.rmtree(...)"
```

或一个已有脚本内部调用 destructive filesystem API，可能绕过 shell admission。

首版不通过复杂解释器审批、脚本静态分析或 OS sandbox 解决这个问题，也不继续扩展 Python / Node 等解释器 denylist 来伪装任意代码 containment。

这是产品威胁模型中的明确限制，不应被文案隐藏；Destructive Guard 只承担少量高置信、常见 destructive primitive 的最后一道缓解，不是 raw execution 的安全沙箱。

---

## 7. 用户交互策略：不做确认弹窗地狱

首版不设计一套“所有高风险操作都问用户”的 approval 系统。

正常开发操作：直接执行。

可由 typed bounded delete 安全表达的目录清理：直接执行。

明显危险且无法安全表达的 destructive raw shell：直接拒绝。

第一版**不提供**：

- “本次仍然允许”按钮；
- “永久关闭保护”开关；
- 连续 shell / interpreter approval；
- 将风险不断甩给用户点击确认的流程。

真的需要被阻止的 destructive operation，用户可以在本机 Terminal 手动执行。

---

## 8. Audit / Diagnostics

被 Guard 拦截时可以记录最小结构化事件：

```text
operation_class = recursive_delete
decision = denied
cwd_scope = project
reason = uncontrolled recursive deletion
```

要求：

- 不把完整原始 command 作为长期日志默认记录；
- 不记录 token、password、Runtime Key、Authorization header；
- 路径如涉及敏感目录只记录安全分类或最小必要信息；
- 用户 UI 显示“阻止了什么类别和为什么”，不展示内部安全实现细节。

---

## 9. CodexLink 自身也遵守相同 destructive 原则

产品代码、installer、migration、staging、test cleanup 均应优先：

```text
known owned file
→ exact delete
→ verify directory empty
→ remove empty directory
```

不得为了方便对未知父目录使用 broad recursive delete。

如果必须递归清理 CodexLink 自己创建的 disposable tree，也必须有：

- exact owned root；
- ownership / sentinel / unique staging identity；
- root / parent protection；
- reparse fail-closed；
- 不触碰用户项目或历史证据目录。

---

## 10. Git-aware classification 的位置

Git-aware classification **不是首版安全依赖**。

未来可选增强：

- Git 可用且当前项目是 repo 时，区分 tracked source / ignored generated / mixed；
- 用于 UI 摘要、审计和模型提示；
- Git 不存在、不是 repo 或命令失败时，自动退回普通 bounded delete；
- 删除硬边界绝不能依赖用户是否安装 Git。

首版可以完全不实现该增强。

---

## 11. Batch F 实现 Gate

Batch F 在恢复 CodexLink-managed model-authored normal shell / process / script execution 前，使用以下 focused Gate；当前本地实现已经通过该 Gate，真实 RC 仍需在 Windows 全链复验：

### Typed filesystem

- create 不覆盖已有文件；
- guarded edit 的 SHA conflict 正常；
- rename 不越界且不覆盖 destination；
- single-file delete 正常；
- deep-directory bounded recursive delete 正常；
- 大目录只单遍递归，不预扫描两遍；
- project / authorization root delete 被拒绝；
- parent / filesystem / drive root delete 被拒绝；
- wildcard target 被拒绝；
- protected anchor 被拒绝；
- junction / symlink / reparse target 和内部 entry fail closed；
- 删除摘要有界且不泄漏无关路径。

### Execution admission

- `cargo test` / `npm test` / 普通 Python / Node / PowerShell script 可通过；
- PowerShell `-EncodedCommand/-enc/-ec/-e` 作为 opaque direct payload 在 spawn 前拒绝；
- obvious recursive delete raw shell 在 spawn 前拒绝；
- destructive Git 在 spawn 前拒绝；
- mirror / purge 在 spawn 前拒绝；
- disk destructive primitive 在 spawn 前拒绝；
- 被拒绝的操作不会留下已启动的 child process。

### Regression

- exact project authority 不回归；
- License / Tunnel secret 不出现在日志；
- Job Object / process ownership 不回归；
- installer / migration / staging 无新增 broad destructive cleanup。

这些 focused tests 已通过后，CodexLink 不修改 WebCodex 通用 enrollment 默认，而是在自己管理的 Runner 上通过 `project activate --allow-raw-shell` + config check / generation CAS / hot reload，从初始 fail-closed 配置切换到“正常 execution + Destructive Guard”。新 RC 真机仍必须验证完整 normal execution 与 deny-no-child-spawn 行为。

---

## 11.5 最终安装包的 tunnel-test 安全验收

Batch F focused tests 证明 Guard/typed contract 的实现，但正式进入规模测试前还要通过当前产品链路做一次真机端到端负面测试：

```text
ChatGPT / tunnel-test
→ 最终 CodexLink bundled Core（记录准确 commit、版本与 installer SHA256）
→ Runner execution admission
→ disposable authorized test root
```

要求：

- 正常 `git status` / build / test / PowerShell / Node / Python 工作流可运行；
- `run_process` 不接受 shell 语义；需要 shell 的命令必须走专用 shell contract；
- PowerShell / cmd / shell 的明显 recursive delete 在 spawn 前拒绝，并复验 encoded-command direct entry；
- destructive Git、mirror/purge、disk destructive primitive 在 spawn 前拒绝；
- project root / parent / drive root / wildcard / protected anchor / reparse 继续 fail closed；junction/reparse TOCTOU 进入后续动态专项，本轮不改写为 handle-based traversal；
- deny 后验证没有 destructive child process 留下；
- typed `delete_directory` 只在专门 disposable tree 中验证成功路径；
- 不对真实 CodexLink 源码树、`D:\Test` 历史证据或用户真实文件做 destructive 负面测试。

这项验证属于最终安装包发布 Gate。公网 Server 切换与旧 WebCodexLauncher 退役属于独立部署决定，不由本规范自动授权。

---

## 12. Windows Sandbox POC 的最终定位

已有 Windows Sandbox POC 是研究证据，不是产品依赖，也不是首版 release Gate。

决策：

- 不要求客户手动启用 Windows Sandbox Optional Feature；
- 不要求因为 CodexLink 安装而重启 Windows；
- 不把 POC 接入 Desktop / Runner production path；
- 主线已通过 `65a97891` 普通 revert 保留历史记录并移除 POC 产品代码；
- 未来如果真实用户反馈要求运行不可信代码，再重新评估可选强隔离模式。

---

## 13. 首版安全能力的诚实定位

首版应该对外表达为：

> CodexLink 尽量不限制正常开发能力，同时对常见的大范围 destructive operation 提供结构化边界和硬拒绝。

不应该表达为：

> CodexLink 是恶意代码 sandbox。

内部评估时，可把首版视为“coding-agent 事故防护能力较强、恶意代码 containment 较弱”的模型。真正的产品验收以本文列出的可测试 Gate 为准，而不是以一个抽象安全分数代替测试。
