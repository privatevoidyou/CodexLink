[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$SshHost,
    [int]$SshPort = 22,
    [string]$SshUser = 'ubuntu',
    [string]$KeyPath = (Join-Path $HOME '.ssh\webcodex-vps'),
    [string]$InstallRoot = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'CodexLink\LicenseOperator'),
    [switch]$NoDesktopShortcut
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($SshHost -notmatch '^[A-Za-z0-9][A-Za-z0-9.:-]*$') {
    throw 'SshHost contains unsupported characters.'
}
if ($SshUser -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
    throw 'SshUser contains unsupported characters.'
}
if ($SshPort -lt 1 -or $SshPort -gt 65535) {
    throw 'SshPort must be between 1 and 65535.'
}

$key = [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($KeyPath))
if (-not (Test-Path -LiteralPath $key -PathType Leaf)) {
    throw "SSH key not found: $key"
}

$sourceScript = Join-Path $PSScriptRoot 'codexlink_license_operator.ps1'
if (-not (Test-Path -LiteralPath $sourceScript -PathType Leaf)) {
    throw "Operator source not found: $sourceScript"
}

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sourceIcon = Join-Path $repoRoot 'apps\desktop\src-tauri\icons\icon.ico'
if (-not (Test-Path -LiteralPath $sourceIcon -PathType Leaf)) {
    throw "CodexLink icon not found: $sourceIcon"
}

$root = [IO.Path]::GetFullPath($InstallRoot)
$appDir = Join-Path $root 'app'
$configPath = Join-Path $root 'config.json'
$installedScript = Join-Path $appDir 'CodexLink-License-Operator.ps1'
$installedIcon = Join-Path $appDir 'CodexLink.ico'
[IO.Directory]::CreateDirectory($appDir) | Out-Null

Copy-Item -LiteralPath $sourceScript -Destination $installedScript -Force
Copy-Item -LiteralPath $sourceIcon -Destination $installedIcon -Force

$config = [ordered]@{
    host = $SshHost
    port = $SshPort
    user = $SshUser
    key_path = $key
} | ConvertTo-Json -Depth 3
$encoding = New-Object System.Text.UTF8Encoding -ArgumentList $false
[IO.File]::WriteAllText($configPath, $config + [Environment]::NewLine, $encoding)

$powershellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
if (-not (Test-Path -LiteralPath $powershellExe -PathType Leaf)) {
    throw "Windows PowerShell not found: $powershellExe"
}

& $powershellExe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $installedScript -SelfTest
if ($LASTEXITCODE -ne 0) {
    throw 'Installed operator self-test failed.'
}

$shortcutPath = ''
if (-not $NoDesktopShortcut) {
    $desktop = [Environment]::GetFolderPath('Desktop')
    if (-not [string]::IsNullOrWhiteSpace($desktop)) {
        $shortcutPath = Join-Path $desktop 'CodexLink 销售码工具.lnk'
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $powershellExe
        $shortcut.Arguments = '-NoLogo -NoProfile -STA -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $installedScript + '" -ConfigPath "' + $configPath + '"'
        $shortcut.WorkingDirectory = $appDir
        $shortcut.IconLocation = $installedIcon + ',0'
        $shortcut.Description = 'CodexLink 管理员销售激活码签发工具'
        $shortcut.Save()
    }
}

Write-Output ('installed_script=' + $installedScript)
Write-Output ('config_path=' + $configPath)
if (-not [string]::IsNullOrWhiteSpace($shortcutPath)) {
    Write-Output ('desktop_shortcut=' + $shortcutPath)
}
