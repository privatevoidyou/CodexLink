# Independent Windows Sandbox filesystem-isolation POC for CodexLink.
#
# This script is intentionally not wired into the Desktop, Runner, or Server.
# It maps exactly one disposable project read/write and one disposable evidence
# directory read/write. Host paths outside those mappings must remain invisible.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ProjectDir,
    [Parameter(Mandatory = $true)][string]$OutputDir,
    [switch]$PrepareOnly,
    [ValidateRange(30, 600)][int]$WaitSeconds = 180
)

$ErrorActionPreference = "Stop"

$ProjectDir = [System.IO.Path]::GetFullPath($ProjectDir)
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)

function Test-PathWithin([string]$Child, [string]$Parent) {
    $parentFull = [System.IO.Path]::GetFullPath($Parent).TrimEnd('\', '/')
    $childFull = [System.IO.Path]::GetFullPath($Child).TrimEnd('\', '/')
    if ([string]::Equals($childFull, $parentFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    return $childFull.StartsWith($parentFull + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)
}

$codexLinkState = Join-Path $env:LOCALAPPDATA "dev.codexlink.desktop"
if ((Test-PathWithin $ProjectDir $codexLinkState) -or (Test-PathWithin $OutputDir $codexLinkState)) {
    throw "Sandbox POC refuses to map CodexLink persistent state"
}
if ((Test-PathWithin $OutputDir $ProjectDir) -or (Test-PathWithin $ProjectDir $OutputDir)) {
    throw "Sandbox POC project and evidence directories must not overlap"
}

if (-not (Test-Path -LiteralPath $ProjectDir -PathType Container)) {
    throw "Sandbox POC project directory does not exist: $ProjectDir"
}
$projectItem = Get-Item -LiteralPath $ProjectDir
if (($projectItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Sandbox POC project directory must not be a reparse point: $ProjectDir"
}
$projectParent = Split-Path -Parent $ProjectDir
if (-not $projectParent -or [string]::Equals($projectParent, $ProjectDir, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Sandbox POC refuses a filesystem root as the project directory"
}
if (Test-Path -LiteralPath $OutputDir) {
    throw "Sandbox POC output already exists: $OutputDir"
}

New-Item -ItemType Directory -Path $OutputDir | Out-Null
$outputItem = Get-Item -LiteralPath $OutputDir
if (($outputItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Sandbox POC output directory must not be a reparse point: $OutputDir"
}

$deniedPaths = @(
    [ordered]@{ label = "project_parent"; path = $projectParent }
)
if (Test-Path -LiteralPath $codexLinkState -PathType Container) {
    $deniedPaths += [ordered]@{ label = "codexlink_state"; path = $codexLinkState }
}
$licenseDir = Join-Path $codexLinkState "license"
if (Test-Path -LiteralPath $licenseDir -PathType Container) {
    $deniedPaths += [ordered]@{ label = "codexlink_license"; path = $licenseDir }
}
$secretDir = Join-Path $codexLinkState "secrets"
if (Test-Path -LiteralPath $secretDir -PathType Container) {
    $deniedPaths += [ordered]@{ label = "codexlink_secrets"; path = $secretDir }
}

$utf8 = [System.Text.UTF8Encoding]::new($false)
$inputPath = Join-Path $OutputDir "probe-input.json"
$input = [ordered]@{
    schema_version = 1
    denied_paths = $deniedPaths
}
[System.IO.File]::WriteAllText($inputPath, ($input | ConvertTo-Json -Depth 5) + "`n", $utf8)

$probePath = Join-Path $OutputDir "probe.ps1"
$probe = @'
$ErrorActionPreference = "Stop"
$project = "C:\CodexLinkProject"
$evidence = "C:\CodexLinkPoc"
$input = Get-Content -LiteralPath (Join-Path $evidence "probe-input.json") -Raw | ConvertFrom-Json

$projectReadOk = Test-Path -LiteralPath $project -PathType Container
$projectWriteOk = $false
$projectCleanupOk = $false
$writeProbe = Join-Path $project ".codexlink-sandbox-write-probe"
if ($projectReadOk) {
    [System.IO.File]::WriteAllText($writeProbe, "sandbox-write-ok`n", [System.Text.Encoding]::ASCII)
    $projectWriteOk = Test-Path -LiteralPath $writeProbe -PathType Leaf
    if ($projectWriteOk) {
        Remove-Item -LiteralPath $writeProbe -Force
        $projectCleanupOk = -not (Test-Path -LiteralPath $writeProbe)
    }
}

$denied = @()
foreach ($probe in @($input.denied_paths)) {
    $denied += [ordered]@{
        label = [string]$probe.label
        visible = [bool](Test-Path -LiteralPath ([string]$probe.path))
    }
}

$networkBlocked = $true
$tcp = [System.Net.Sockets.TcpClient]::new()
try {
    $connect = $tcp.ConnectAsync("1.1.1.1", 443)
    if ($connect.Wait(1500) -and $tcp.Connected) {
        $networkBlocked = $false
    }
} catch {
    $networkBlocked = $true
} finally {
    $tcp.Dispose()
}

$childCancelled = $false
$child = Start-Process -FilePath "$env:SystemRoot\System32\ping.exe" -ArgumentList @("-t", "127.0.0.1") -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Milliseconds 250
    Stop-Process -Id $child.Id -Force
    $null = $child.WaitForExit(5000)
    $childCancelled = $child.HasExited
} finally {
    if (-not $child.HasExited) {
        Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue
    }
    $child.Dispose()
}

$deniedOk = @($denied | Where-Object { $_.visible }).Count -eq 0
$passed = $projectReadOk -and $projectWriteOk -and $projectCleanupOk -and $deniedOk -and $networkBlocked -and $childCancelled
$result = [ordered]@{
    schema_version = 1
    passed = [bool]$passed
    project_read_ok = [bool]$projectReadOk
    project_write_ok = [bool]$projectWriteOk
    project_cleanup_ok = [bool]$projectCleanupOk
    denied_paths = $denied
    network_blocked = [bool]$networkBlocked
    child_cancelled = [bool]$childCancelled
}
$resultPath = Join-Path $evidence "probe-result.json"
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($resultPath, ($result | ConvertTo-Json -Depth 5) + "`n", $utf8)

Start-Process -FilePath "$env:SystemRoot\System32\shutdown.exe" -ArgumentList @("/s", "/t", "0") -WindowStyle Hidden
'@
[System.IO.File]::WriteAllText($probePath, $probe + "`n", $utf8)

$escapedProject = [System.Security.SecurityElement]::Escape($ProjectDir)
$escapedOutput = [System.Security.SecurityElement]::Escape($OutputDir)
$config = @"
<Configuration>
  <VGpu>Disable</VGpu>
  <Networking>Disable</Networking>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <ProtectedClient>Enable</ProtectedClient>
  <PrinterRedirection>Disable</PrinterRedirection>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MemoryInMB>4096</MemoryInMB>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$escapedProject</HostFolder>
      <SandboxFolder>C:\CodexLinkProject</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$escapedOutput</HostFolder>
      <SandboxFolder>C:\CodexLinkPoc</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\CodexLinkPoc\probe.ps1</Command>
  </LogonCommand>
</Configuration>
"@
$configPath = Join-Path $OutputDir "codexlink-sandbox-poc.wsb"
[System.IO.File]::WriteAllText($configPath, $config.Trim() + "`n", $utf8)

# Parse the generated XML before any attempt to launch Windows Sandbox.
$xml = [xml](Get-Content -LiteralPath $configPath -Raw)
if ($xml.Configuration.MappedFolders.MappedFolder.Count -ne 2) {
    throw "Sandbox POC generated an unexpected mapped-folder count"
}
if ($xml.Configuration.Networking -ne "Disable" -or $xml.Configuration.ClipboardRedirection -ne "Disable") {
    throw "Sandbox POC generated an unsafe redirection/network configuration"
}

Write-Output "Sandbox POC prepared: $configPath"
if ($PrepareOnly) {
    Write-Output "Prepare-only mode: Windows Sandbox was not launched"
    exit 0
}

$sandboxExe = Join-Path $env:WINDIR "System32\WindowsSandbox.exe"
if (-not (Test-Path -LiteralPath $sandboxExe -PathType Leaf)) {
    throw "Windows Sandbox is not available on this machine. Enable the Windows Sandbox optional feature manually, then rerun this POC."
}

$resultPath = Join-Path $OutputDir "probe-result.json"
$sandboxProcess = Start-Process -FilePath $sandboxExe -ArgumentList @("`"$configPath`"") -PassThru
$deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
while (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
    if ([DateTime]::UtcNow -ge $deadline) {
        throw "Windows Sandbox POC timed out waiting for probe-result.json; evidence preserved at $OutputDir"
    }
    if ($sandboxProcess.HasExited) {
        throw "Windows Sandbox exited before producing probe-result.json; evidence preserved at $OutputDir"
    }
    Start-Sleep -Milliseconds 500
}

$result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
if (-not $result.passed) {
    throw "Windows Sandbox POC failed one or more isolation assertions; inspect $resultPath"
}
Write-Output "Sandbox POC passed: project read/write, denied host paths, networking disabled, child cancellation"
Write-Output "Evidence: $resultPath"
