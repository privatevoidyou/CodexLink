[CmdletBinding()]
param(
    [string]$ConfigPath = "",
    [switch]$SelfTest,
    [switch]$ProbeConnection
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:ActivationCodePattern = '^MYWS-[A-Z0-9]{5}(?:-[A-Z0-9]{5}){3}$'
$script:RemoteIssueCommand = '/usr/local/bin/myws-license-admin issue --json'
$script:IssueTimeoutMs = 25000
$script:IssueState = $null

function Get-DefaultConfigPath {
    $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
    if ([string]::IsNullOrWhiteSpace($localAppData)) {
        throw 'Unable to resolve LocalApplicationData.'
    }
    return (Join-Path $localAppData 'CodexLink\LicenseOperator\config.json')
}

function Get-EffectiveConfigPath {
    param([string]$RequestedPath)

    if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
        return [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($RequestedPath))
    }
    return (Get-DefaultConfigPath)
}

function Get-DefaultOperatorConfig {
    $defaultKey = Join-Path $HOME '.ssh\webcodex-vps'
    return [pscustomobject]@{
        host = ''
        port = 22
        user = 'ubuntu'
        key_path = $defaultKey
    }
}

function Get-JsonPropertyValue {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        $DefaultValue
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property -or $null -eq $property.Value) {
        return $DefaultValue
    }
    return $property.Value
}

function Read-OperatorConfig {
    param([Parameter(Mandatory = $true)][string]$Path)

    $defaults = Get-DefaultOperatorConfig
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $defaults
    }

    try {
        $raw = [IO.File]::ReadAllText($Path)
        $parsed = $raw | ConvertFrom-Json
        return [pscustomobject]@{
            host = [string](Get-JsonPropertyValue -Object $parsed -Name 'host' -DefaultValue $defaults.host)
            port = [int](Get-JsonPropertyValue -Object $parsed -Name 'port' -DefaultValue $defaults.port)
            user = [string](Get-JsonPropertyValue -Object $parsed -Name 'user' -DefaultValue $defaults.user)
            key_path = [string](Get-JsonPropertyValue -Object $parsed -Name 'key_path' -DefaultValue $defaults.key_path)
        }
    }
    catch {
        throw "连接配置无法读取：$($_.Exception.Message)"
    }
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        [IO.Directory]::CreateDirectory($parent) | Out-Null
    }
    $encoding = New-Object System.Text.UTF8Encoding -ArgumentList $false
    [IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Save-OperatorConfig {
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $normalized = Normalize-OperatorConfig -Config $Config
    $document = [ordered]@{
        host = $normalized.host
        port = $normalized.port
        user = $normalized.user
        key_path = $normalized.key_path
    } | ConvertTo-Json -Depth 3
    Write-Utf8NoBom -Path $Path -Text ($document + [Environment]::NewLine)
    return $normalized
}

function Resolve-KeyPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $expanded = [Environment]::ExpandEnvironmentVariables($Path.Trim())
    if ($expanded -eq '~') {
        $expanded = $HOME
    }
    elseif ($expanded.StartsWith('~\') -or $expanded.StartsWith('~/')) {
        $expanded = Join-Path $HOME $expanded.Substring(2)
    }
    return [IO.Path]::GetFullPath($expanded)
}

function Normalize-OperatorConfig {
    param([Parameter(Mandatory = $true)]$Config)

    $sshHost = ([string]$Config.host).Trim()
    $sshUser = ([string]$Config.user).Trim()
    $keyPath = Resolve-KeyPath -Path ([string]$Config.key_path)
    $port = [int]$Config.port

    if ([string]::IsNullOrWhiteSpace($sshHost)) {
        throw '请先填写 SSH 服务器地址。'
    }
    if ($sshHost -notmatch '^[A-Za-z0-9][A-Za-z0-9.:-]*$') {
        throw 'SSH 服务器地址包含不支持的字符。'
    }
    if ([string]::IsNullOrWhiteSpace($sshUser) -or $sshUser -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        throw 'SSH 用户名格式无效。'
    }
    if ($port -lt 1 -or $port -gt 65535) {
        throw 'SSH 端口必须在 1 到 65535 之间。'
    }
    if (-not (Test-Path -LiteralPath $keyPath -PathType Leaf)) {
        throw "找不到 SSH 私钥：$keyPath"
    }

    return [pscustomobject]@{
        host = $sshHost
        port = $port
        user = $sshUser
        key_path = $keyPath
    }
}

function Resolve-SshExecutable {
    $windowsSsh = Join-Path $env:WINDIR 'System32\OpenSSH\ssh.exe'
    if (Test-Path -LiteralPath $windowsSsh -PathType Leaf) {
        return $windowsSsh
    }

    $command = Get-Command 'ssh.exe' -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }
    throw '未找到 Windows OpenSSH (ssh.exe)。请先在“可选功能”中安装 OpenSSH Client。'
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Value)

    if ($Value.Contains('"')) {
        throw 'Process argument unexpectedly contains a quote character.'
    }
    if ($Value.EndsWith('\')) {
        throw 'Process argument unexpectedly ends with a backslash.'
    }
    return '"' + $Value + '"'
}

function Join-ProcessArguments {
    param([Parameter(Mandatory = $true)][object[]]$Values)
    return (($Values | ForEach-Object { ConvertTo-ProcessArgument -Value ([string]$_) }) -join ' ')
}

function Start-SshCommand {
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)][string]$RemoteCommand
    )

    $normalized = Normalize-OperatorConfig -Config $Config
    $sshExe = Resolve-SshExecutable
    $target = $normalized.user + '@' + $normalized.host
    $arguments = @(
        '-T',
        '-i', $normalized.key_path,
        '-p', [string]$normalized.port,
        '-o', 'BatchMode=yes',
        '-o', 'PasswordAuthentication=no',
        '-o', 'KbdInteractiveAuthentication=no',
        '-o', 'PreferredAuthentications=publickey',
        '-o', 'ConnectionAttempts=1',
        '-o', 'ConnectTimeout=10',
        '-o', 'ServerAliveInterval=5',
        '-o', 'ServerAliveCountMax=2',
        '-o', 'StrictHostKeyChecking=yes',
        '-o', 'LogLevel=ERROR',
        $target,
        $RemoteCommand
    )

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $sshExe
    $startInfo.Arguments = Join-ProcessArguments -Values $arguments
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw '无法启动 ssh.exe。'
    }

    # Windows OpenSSH can misbehave when a GUI process has no valid console stdin.
    # Redirect stdin and close it immediately so SSH gets a real EOF without prompting.
    $process.StandardInput.Close()

    return [pscustomobject]@{
        Process = $process
        StdoutTask = $process.StandardOutput.ReadToEndAsync()
        StderrTask = $process.StandardError.ReadToEndAsync()
        StartedUtc = [DateTime]::UtcNow
    }
}

function Stop-SshState {
    param($State)

    if ($null -eq $State -or $null -eq $State.Process) {
        return
    }
    try {
        if (-not $State.Process.HasExited) {
            $State.Process.Kill()
            [void]$State.Process.WaitForExit(3000)
        }
    }
    catch {
        # Best effort only. The SSH request must never be retried automatically here.
    }
}

function Get-TaskText {
    param($Task)
    if ($null -eq $Task) {
        return ''
    }
    return [string]$Task.GetAwaiter().GetResult()
}

function Get-SafeSshError {
    param(
        [string]$Stderr,
        [int]$ExitCode,
        [bool]$TimedOut
    )

    $filtered = @()
    foreach ($line in ($Stderr -split "`r?`n")) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed)) { continue }
        if ($trimmed -match 'MYWS_LICENSE_ISSUE') { continue }
        if ($trimmed -match '^Cmnd_Alias\s+') { continue }
        if ($trimmed -match '^\^+$') { continue }
        if ($trimmed -match '^GetConsoleMode on STD_INPUT_HANDLE failed') { continue }
        $filtered += $trimmed
    }

    if ($filtered.Count -gt 0) {
        return (($filtered | Select-Object -Last 4) -join [Environment]::NewLine)
    }
    if ($TimedOut) {
        return 'SSH 操作超时，且没有收到有效销售码。请检查网络后手动重试。'
    }
    return "SSH 操作失败（exit $ExitCode）。"
}

function Get-IssueRecordFromStdout {
    param([Parameter(Mandatory = $true)][string]$Stdout)

    $records = @()
    foreach ($line in ($Stdout -split "`r?`n")) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed)) { continue }

        try {
            $parsed = $trimmed | ConvertFrom-Json
        }
        catch {
            continue
        }

        $codeProperty = $parsed.PSObject.Properties['code']
        if ($null -eq $codeProperty) { continue }
        $code = ([string]$codeProperty.Value).Trim().ToUpperInvariant()
        if ($code -notmatch $script:ActivationCodePattern) { continue }

        $id = ''
        $idProperty = $parsed.PSObject.Properties['activation_code_id']
        if ($null -ne $idProperty -and $null -ne $idProperty.Value) {
            $id = ([string]$idProperty.Value).Trim()
        }
        $records += [pscustomobject]@{
            code = $code
            activation_code_id = $id
        }
    }

    $uniqueCodes = @($records | Group-Object -Property code)
    if ($uniqueCodes.Count -eq 0) {
        throw '服务器没有返回有效的销售激活码 JSON。'
    }
    if ($uniqueCodes.Count -ne 1) {
        throw '服务器一次返回了多枚不同的销售激活码；为避免误发，工具已停止。'
    }
    return $records[0]
}

function Invoke-SshCommandSync {
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)][string]$RemoteCommand,
        [int]$TimeoutMs = 15000
    )

    $state = Start-SshCommand -Config $Config -RemoteCommand $RemoteCommand
    $timedOut = $false
    try {
        if (-not $state.Process.WaitForExit($TimeoutMs)) {
            $timedOut = $true
            Stop-SshState -State $state
        }
        else {
            [void]$state.Process.WaitForExit()
        }

        $stdout = Get-TaskText -Task $state.StdoutTask
        $stderr = Get-TaskText -Task $state.StderrTask
        $exitCode = if ($state.Process.HasExited) { $state.Process.ExitCode } else { -1 }
        if ($timedOut -or $exitCode -ne 0) {
            throw (Get-SafeSshError -Stderr $stderr -ExitCode $exitCode -TimedOut $timedOut)
        }
        return $stdout
    }
    finally {
        Stop-SshState -State $state
        $state.Process.Dispose()
    }
}

function Invoke-SelfTest {
    $sample = '{"code":"MYWS-ABCDE-FGHIJ-KLMNO-PQRST","activation_code_id":"00000000-0000-0000-0000-000000000001"}'
    $record = Get-IssueRecordFromStdout -Stdout $sample
    if ($record.code -ne 'MYWS-ABCDE-FGHIJ-KLMNO-PQRST') {
        throw 'activation-code parse self-test failed'
    }

    $invalidRejected = $false
    try {
        [void](Get-IssueRecordFromStdout -Stdout '{"code":"INVALID"}')
    }
    catch {
        $invalidRejected = $true
    }
    if (-not $invalidRejected) {
        throw 'invalid activation-code self-test failed'
    }

    $quoted = ConvertTo-ProcessArgument -Value 'C:\Program Files\CodexLink\key'
    if ($quoted -ne '"C:\Program Files\CodexLink\key"') {
        throw 'process argument quoting self-test failed'
    }

    Write-Output 'SELFTEST_OK'
}

function Show-SettingsDialog {
    param(
        [Parameter(Mandatory = $true)][System.Windows.Forms.IWin32Window]$Owner,
        [Parameter(Mandatory = $true)]$CurrentConfig,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $dialog = New-Object System.Windows.Forms.Form
    $dialog.Text = '连接设置'
    $dialog.StartPosition = 'CenterParent'
    $dialog.FormBorderStyle = 'FixedDialog'
    $dialog.MaximizeBox = $false
    $dialog.MinimizeBox = $false
    $dialog.ClientSize = New-Object System.Drawing.Size(540, 300)
    $dialog.BackColor = [System.Drawing.Color]::FromArgb(248, 250, 253)
    $dialog.Font = New-Object System.Drawing.Font('Segoe UI', 9)

    $labelHost = New-Object System.Windows.Forms.Label
    $labelHost.Text = 'SSH 服务器'
    $labelHost.SetBounds(24, 24, 120, 22)
    $dialog.Controls.Add($labelHost)

    $textHost = New-Object System.Windows.Forms.TextBox
    $textHost.Text = [string]$CurrentConfig.host
    $textHost.SetBounds(150, 22, 250, 26)
    $dialog.Controls.Add($textHost)

    $textPort = New-Object System.Windows.Forms.NumericUpDown
    $textPort.Minimum = 1
    $textPort.Maximum = 65535
    $textPort.Value = [decimal][int]$CurrentConfig.port
    $textPort.SetBounds(414, 22, 100, 26)
    $dialog.Controls.Add($textPort)

    $labelUser = New-Object System.Windows.Forms.Label
    $labelUser.Text = 'SSH 用户'
    $labelUser.SetBounds(24, 66, 120, 22)
    $dialog.Controls.Add($labelUser)

    $textUser = New-Object System.Windows.Forms.TextBox
    $textUser.Text = [string]$CurrentConfig.user
    $textUser.SetBounds(150, 64, 364, 26)
    $dialog.Controls.Add($textUser)

    $labelKey = New-Object System.Windows.Forms.Label
    $labelKey.Text = 'SSH 私钥'
    $labelKey.SetBounds(24, 108, 120, 22)
    $dialog.Controls.Add($labelKey)

    $textKey = New-Object System.Windows.Forms.TextBox
    $textKey.Text = [string]$CurrentConfig.key_path
    $textKey.SetBounds(150, 106, 274, 26)
    $dialog.Controls.Add($textKey)

    $buttonBrowse = New-Object System.Windows.Forms.Button
    $buttonBrowse.Text = '浏览...'
    $buttonBrowse.SetBounds(434, 104, 80, 30)
    $buttonBrowse.Add_Click({
        $picker = New-Object System.Windows.Forms.OpenFileDialog
        $picker.Title = '选择 OpenSSH 私钥'
        $picker.CheckFileExists = $true
        $picker.Filter = '所有文件 (*.*)|*.*'
        if ($picker.ShowDialog($dialog) -eq [System.Windows.Forms.DialogResult]::OK) {
            $textKey.Text = $picker.FileName
        }
        $picker.Dispose()
    })
    $dialog.Controls.Add($buttonBrowse)

    $help = New-Object System.Windows.Forms.Label
    $help.Text = '只保存服务器地址、用户名和私钥路径。私钥文件本身不会被复制到工具目录。'
    $help.ForeColor = [System.Drawing.Color]::FromArgb(100, 116, 139)
    $help.SetBounds(24, 152, 490, 44)
    $dialog.Controls.Add($help)

    $buttonCancel = New-Object System.Windows.Forms.Button
    $buttonCancel.Text = '取消'
    $buttonCancel.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
    $buttonCancel.SetBounds(344, 228, 80, 34)
    $dialog.Controls.Add($buttonCancel)

    $buttonSave = New-Object System.Windows.Forms.Button
    $buttonSave.Text = '保存'
    $buttonSave.SetBounds(434, 228, 80, 34)
    $buttonSave.BackColor = [System.Drawing.Color]::FromArgb(59, 91, 246)
    $buttonSave.ForeColor = [System.Drawing.Color]::White
    $buttonSave.FlatStyle = 'Flat'
    $buttonSave.FlatAppearance.BorderSize = 0
    $buttonSave.Add_Click({
        try {
            $candidate = [pscustomobject]@{
                host = $textHost.Text
                port = [int]$textPort.Value
                user = $textUser.Text
                key_path = $textKey.Text
            }
            [void](Save-OperatorConfig -Config $candidate -Path $Path)
            $dialog.DialogResult = [System.Windows.Forms.DialogResult]::OK
            $dialog.Close()
        }
        catch {
            [System.Windows.Forms.MessageBox]::Show(
                $dialog,
                $_.Exception.Message,
                '连接设置无效',
                [System.Windows.Forms.MessageBoxButtons]::OK,
                [System.Windows.Forms.MessageBoxIcon]::Warning
            ) | Out-Null
        }
    })
    $dialog.Controls.Add($buttonSave)

    $dialog.AcceptButton = $buttonSave
    $dialog.CancelButton = $buttonCancel
    $result = $dialog.ShowDialog($Owner)
    $dialog.Dispose()
    return $result
}

function Show-LicenseOperatorWindow {
    param([Parameter(Mandatory = $true)][string]$Path)

    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    [System.Windows.Forms.Application]::EnableVisualStyles()

    $form = New-Object System.Windows.Forms.Form
    $form.Text = 'CodexLink 销售码工具'
    $form.StartPosition = 'CenterScreen'
    $form.ClientSize = New-Object System.Drawing.Size(640, 500)
    $form.MinimumSize = New-Object System.Drawing.Size(656, 539)
    $form.BackColor = [System.Drawing.Color]::FromArgb(245, 247, 251)
    $form.Font = New-Object System.Drawing.Font('Segoe UI', 9)

    $iconPath = Join-Path $PSScriptRoot 'CodexLink.ico'
    if (Test-Path -LiteralPath $iconPath -PathType Leaf) {
        try { $form.Icon = New-Object System.Drawing.Icon($iconPath) } catch { }
    }

    $panel = New-Object System.Windows.Forms.Panel
    $panel.BackColor = [System.Drawing.Color]::White
    $panel.SetBounds(36, 34, 568, 424)
    $panel.BorderStyle = 'FixedSingle'
    $form.Controls.Add($panel)

    $title = New-Object System.Windows.Forms.Label
    $title.Text = 'CodexLink 销售码工具'
    $title.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 17)
    $title.ForeColor = [System.Drawing.Color]::FromArgb(15, 23, 42)
    $title.SetBounds(28, 26, 380, 36)
    $panel.Controls.Add($title)

    $subtitle = New-Object System.Windows.Forms.Label
    $subtitle.Text = '管理员工具 · 签名私钥始终留在授权服务器'
    $subtitle.ForeColor = [System.Drawing.Color]::FromArgb(100, 116, 139)
    $subtitle.SetBounds(30, 64, 420, 24)
    $panel.Controls.Add($subtitle)

    $buttonSettings = New-Object System.Windows.Forms.Button
    $buttonSettings.Text = '连接设置'
    $buttonSettings.SetBounds(448, 28, 90, 32)
    $panel.Controls.Add($buttonSettings)

    $separator = New-Object System.Windows.Forms.Label
    $separator.BorderStyle = 'Fixed3D'
    $separator.SetBounds(28, 102, 510, 2)
    $panel.Controls.Add($separator)

    $labelCode = New-Object System.Windows.Forms.Label
    $labelCode.Text = '最新销售激活码'
    $labelCode.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 10)
    $labelCode.SetBounds(28, 126, 200, 26)
    $panel.Controls.Add($labelCode)

    $textCode = New-Object System.Windows.Forms.TextBox
    $textCode.ReadOnly = $true
    $textCode.Text = '尚未生成'
    $textCode.Font = New-Object System.Drawing.Font('Consolas', 12)
    $textCode.BackColor = [System.Drawing.Color]::FromArgb(248, 250, 252)
    $textCode.SetBounds(28, 158, 390, 34)
    $panel.Controls.Add($textCode)

    $buttonCopy = New-Object System.Windows.Forms.Button
    $buttonCopy.Text = '复制'
    $buttonCopy.Enabled = $false
    $buttonCopy.SetBounds(430, 156, 108, 38)
    $panel.Controls.Add($buttonCopy)

    $buttonGenerate = New-Object System.Windows.Forms.Button
    $buttonGenerate.Text = '生成新的销售激活码'
    $buttonGenerate.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 10)
    $buttonGenerate.BackColor = [System.Drawing.Color]::FromArgb(59, 91, 246)
    $buttonGenerate.ForeColor = [System.Drawing.Color]::White
    $buttonGenerate.FlatStyle = 'Flat'
    $buttonGenerate.FlatAppearance.BorderSize = 0
    $buttonGenerate.SetBounds(28, 220, 220, 44)
    $panel.Controls.Add($buttonGenerate)

    $status = New-Object System.Windows.Forms.Label
    $status.Text = '准备就绪'
    $status.ForeColor = [System.Drawing.Color]::FromArgb(71, 85, 105)
    $status.SetBounds(28, 282, 510, 44)
    $panel.Controls.Add($status)

    $connection = New-Object System.Windows.Forms.Label
    $connection.ForeColor = [System.Drawing.Color]::FromArgb(100, 116, 139)
    $connection.SetBounds(28, 330, 510, 24)
    $panel.Controls.Add($connection)

    $notice = New-Object System.Windows.Forms.Label
    $notice.Text = '每次点击都会在服务器创建一枚新的 unused 销售码。签发请求不会自动重试；销售码不会写入本地日志或配置文件。'
    $notice.ForeColor = [System.Drawing.Color]::FromArgb(146, 64, 14)
    $notice.BackColor = [System.Drawing.Color]::FromArgb(255, 247, 237)
    $notice.SetBounds(28, 366, 510, 42)
    $panel.Controls.Add($notice)

    function Update-ConnectionSummary {
        try {
            $cfg = Read-OperatorConfig -Path $Path
            if ([string]::IsNullOrWhiteSpace([string]$cfg.host)) {
                $connection.Text = '连接：尚未配置'
            }
            else {
                $connection.Text = "连接：$($cfg.user)@$($cfg.host):$($cfg.port) · $([IO.Path]::GetFileName([string]$cfg.key_path))"
            }
        }
        catch {
            $connection.Text = '连接：配置需要修复'
        }
    }

    function Set-IdleState {
        $buttonGenerate.Enabled = $true
        $buttonSettings.Enabled = $true
        $buttonGenerate.Text = '生成新的销售激活码'
    }

    function Copy-CodeToClipboard {
        if ($textCode.Text -notmatch $script:ActivationCodePattern) {
            return $false
        }
        try {
            [System.Windows.Forms.Clipboard]::SetText($textCode.Text)
            return $true
        }
        catch {
            try {
                Set-Clipboard -Value $textCode.Text
                return $true
            }
            catch {
                return $false
            }
        }
    }

    $timer = New-Object System.Windows.Forms.Timer
    $timer.Interval = 120
    $timer.Add_Tick({
        if ($null -eq $script:IssueState) {
            $timer.Stop()
            return
        }

        $state = $script:IssueState
        $timedOut = (([DateTime]::UtcNow - $state.StartedUtc).TotalMilliseconds -ge $script:IssueTimeoutMs)
        $hasExited = $false
        try { $hasExited = $state.Process.HasExited } catch { $hasExited = $true }
        if (-not $hasExited -and -not $timedOut) {
            return
        }

        $timer.Stop()
        try {
            if ($timedOut -and -not $hasExited) {
                Stop-SshState -State $state
            }
            elseif ($hasExited) {
                [void]$state.Process.WaitForExit()
            }

            $stdout = Get-TaskText -Task $state.StdoutTask
            $stderr = Get-TaskText -Task $state.StderrTask
            $exitCode = if ($state.Process.HasExited) { $state.Process.ExitCode } else { -1 }

            # A valid issued code is authoritative because issue is non-idempotent. Never retry it
            # merely because SSH cleanup timed out or stderr contains a non-fatal sudoers warning.
            $record = $null
            try { $record = Get-IssueRecordFromStdout -Stdout $stdout } catch { }
            if ($null -ne $record) {
                $textCode.Text = $record.code
                $buttonCopy.Enabled = $true
                if (Copy-CodeToClipboard) {
                    if ($timedOut -or $exitCode -ne 0) {
                        $status.Text = '签发成功，已复制到剪贴板。SSH 会话已由本地安全收尾；不要重复点击生成。'
                    }
                    else {
                        $status.Text = '签发成功，已复制到剪贴板。'
                    }
                }
                else {
                    $status.Text = '签发成功。自动复制失败，请点击“复制”。'
                }
                $status.ForeColor = [System.Drawing.Color]::FromArgb(5, 150, 105)
            }
            else {
                $message = Get-SafeSshError -Stderr $stderr -ExitCode $exitCode -TimedOut $timedOut
                $status.Text = $message
                $status.ForeColor = [System.Drawing.Color]::FromArgb(185, 28, 28)
            }
        }
        catch {
            $status.Text = $_.Exception.Message
            $status.ForeColor = [System.Drawing.Color]::FromArgb(185, 28, 28)
        }
        finally {
            Stop-SshState -State $state
            try { $state.Process.Dispose() } catch { }
            $script:IssueState = $null
            Set-IdleState
        }
    })

    $buttonCopy.Add_Click({
        if (Copy-CodeToClipboard) {
            $status.Text = '已重新复制到剪贴板。'
            $status.ForeColor = [System.Drawing.Color]::FromArgb(5, 150, 105)
        }
        else {
            $status.Text = '复制失败，请手动选中激活码复制。'
            $status.ForeColor = [System.Drawing.Color]::FromArgb(185, 28, 28)
        }
    })

    $buttonSettings.Add_Click({
        if ($null -ne $script:IssueState) { return }
        $current = Read-OperatorConfig -Path $Path
        if ((Show-SettingsDialog -Owner $form -CurrentConfig $current -Path $Path) -eq [System.Windows.Forms.DialogResult]::OK) {
            Update-ConnectionSummary
            $status.Text = '连接设置已保存。'
            $status.ForeColor = [System.Drawing.Color]::FromArgb(71, 85, 105)
        }
    })

    $buttonGenerate.Add_Click({
        if ($null -ne $script:IssueState) { return }

        try {
            $config = Normalize-OperatorConfig -Config (Read-OperatorConfig -Path $Path)
        }
        catch {
            [System.Windows.Forms.MessageBox]::Show(
                $form,
                $_.Exception.Message + [Environment]::NewLine + [Environment]::NewLine + '请先打开“连接设置”。',
                '无法生成销售码',
                [System.Windows.Forms.MessageBoxButtons]::OK,
                [System.Windows.Forms.MessageBoxIcon]::Warning
            ) | Out-Null
            return
        }

        try {
            $script:IssueState = Start-SshCommand -Config $config -RemoteCommand $script:RemoteIssueCommand
            $buttonGenerate.Enabled = $false
            $buttonSettings.Enabled = $false
            $buttonGenerate.Text = '正在签发...'
            $status.Text = '正在通过 SSH 请求授权服务器签发 1 枚销售激活码...'
            $status.ForeColor = [System.Drawing.Color]::FromArgb(71, 85, 105)
            $timer.Start()
        }
        catch {
            $script:IssueState = $null
            $status.Text = $_.Exception.Message
            $status.ForeColor = [System.Drawing.Color]::FromArgb(185, 28, 28)
            Set-IdleState
        }
    })

    $form.Add_FormClosing({
        $timer.Stop()
        if ($null -ne $script:IssueState) {
            Stop-SshState -State $script:IssueState
            try { $script:IssueState.Process.Dispose() } catch { }
            $script:IssueState = $null
        }
    })

    Update-ConnectionSummary
    $form.Add_Shown({
        $current = Read-OperatorConfig -Path $Path
        if ([string]::IsNullOrWhiteSpace([string]$current.host)) {
            [void](Show-SettingsDialog -Owner $form -CurrentConfig $current -Path $Path)
            Update-ConnectionSummary
        }
    })

    [void]$form.ShowDialog()
    $timer.Dispose()
    $form.Dispose()
}

$effectiveConfigPath = Get-EffectiveConfigPath -RequestedPath $ConfigPath

if ($SelfTest) {
    try {
        Invoke-SelfTest
        exit 0
    }
    catch {
        Write-Error $_.Exception.Message
        exit 1
    }
}

if ($ProbeConnection) {
    try {
        $config = Normalize-OperatorConfig -Config (Read-OperatorConfig -Path $effectiveConfigPath)
        $probe = Invoke-SshCommandSync -Config $config -RemoteCommand 'printf CODEXLINK_LICENSE_OPERATOR_OK' -TimeoutMs 15000
        if ($probe.Trim() -ne 'CODEXLINK_LICENSE_OPERATOR_OK') {
            throw 'SSH probe returned an unexpected response.'
        }
        Write-Output 'PROBE_OK'
        exit 0
    }
    catch {
        Write-Error $_.Exception.Message
        exit 1
    }
}

try {
    Show-LicenseOperatorWindow -Path $effectiveConfigPath
}
catch {
    try {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show(
            $_.Exception.Message,
            'CodexLink 销售码工具',
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Error
        ) | Out-Null
    }
    catch {
        Write-Error $_.Exception.Message
    }
    exit 1
}
