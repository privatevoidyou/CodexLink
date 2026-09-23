# Install, inspect, and uninstall one newly-built CodexLink NSIS package.
# The helper never kills by executable name and proves uninstall preserves CodexLink user state.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$InstallDir,
    [Parameter(Mandatory = $true)][string]$ProductVersion,
    [Parameter(Mandatory = $true)][string]$EngineVersion,
    [Parameter(Mandatory = $true)][string]$SourceSha,
    [Parameter(Mandatory = $true)][Int64]$BuiltAt
)

$ErrorActionPreference = "Stop"
if ($ProductVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "invalid CodexLink product version '$ProductVersion'"
}
if ($EngineVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "invalid WebCodex Engine version '$EngineVersion'"
}
$Installer = [System.IO.Path]::GetFullPath($Installer)
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)
if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) {
    throw "Desktop installer does not exist: $Installer"
}
if (Test-Path -LiteralPath $InstallDir) {
    throw "refusing Desktop installer smoke because custom install path already exists: $InstallDir"
}
if ($SourceSha -notmatch '^[0-9A-Fa-f]{40}$') {
    throw "SourceSha must be one exact 40-hex Git commit"
}
if ($BuiltAt -le 0) {
    throw "BuiltAt must be a positive Unix timestamp"
}

function Get-CodexLinkUninstallEntry {
    $root = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
    if (-not (Test-Path -LiteralPath $root)) { return $null }
    $entries = @(
        Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue } |
            Where-Object { $_.DisplayName -eq "CodexLink" }
    )
    if ($entries.Count -gt 1) {
        throw "multiple current-user CodexLink uninstall entries found"
    }
    return @($entries | Select-Object -First 1)[0]
}

function Resolve-UninstallExecutable([string]$command) {
    if (-not $command) { throw "CodexLink uninstall command is missing" }
    $match = [regex]::Match($command, '^\s*"([^"]+)"')
    if ($match.Success) { return $match.Groups[1].Value }
    return ($command -split '\s+', 2)[0]
}

function Resolve-RegistryPath([string]$value, [string]$field) {
    if (-not $value) { throw "CodexLink $field is missing" }
    $trimmed = $value.Trim()
    if ($trimmed.StartsWith('"') -or $trimmed.EndsWith('"')) {
        if (-not ($trimmed.StartsWith('"') -and $trimmed.EndsWith('"') -and $trimmed.Length -ge 2)) {
            throw "CodexLink $field has malformed quoting"
        }
        $trimmed = $trimmed.Substring(1, $trimmed.Length - 2)
    }
    if (-not $trimmed -or $trimmed.Contains('"')) {
        throw "CodexLink $field is not one executable-system path"
    }
    return [System.IO.Path]::GetFullPath($trimmed)
}

function Get-VersionLine([string]$binary, [string]$name) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.Arguments = "--version"
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) {
            throw "$name.exe --version could not start after Desktop install"
        }
        $stdout = $process.StandardOutput.ReadToEnd()
        $null = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) {
            throw "$name.exe --version failed after Desktop install with exit code $($process.ExitCode)"
        }
        $lines = @($stdout -split "`r?`n" | Where-Object { $_ -ne "" })
        if ($lines.Count -ne 1) {
            throw "$name.exe --version returned an unexpected line count after Desktop install"
        }
        return $lines[0].TrimEnd()
    } finally {
        $process.Dispose()
    }
}

function Assert-BundledProductMode([string]$binary, [string]$name) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.Arguments = "--codexlink-bundle-verification"
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "$name bundled admission verification could not start" }
        $null = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        if ($process.ExitCode -ne 78 -or -not $stderr.Contains("bundled CodexLink Core requires an activated CodexLink Desktop")) {
            throw "$name is not a fail-closed CodexLink bundled product-mode binary"
        }
    } finally {
        $process.Dispose()
    }
}

function Assert-StandaloneCoreBlocked([string]$binary) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary
    $start.Arguments = "server status --url http://127.0.0.1:1 --json --no-system-proxy"
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "standalone CodexLink Core admission smoke could not start" }
        $null = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        if ($process.ExitCode -eq 0) {
            throw "bundled CodexLink Core unexpectedly allowed a standalone operational command"
        }
        if (-not $stderr.Contains("bundled CodexLink Core requires an activated CodexLink Desktop")) {
            throw "bundled CodexLink Core failed for an unexpected reason: $stderr"
        }
    } finally {
        $process.Dispose()
    }
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds, [string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $Failure
}

if (Get-CodexLinkUninstallEntry) {
    throw "refusing Desktop installer smoke because CodexLink is already installed for this user"
}

$stateDir = Join-Path $env:LOCALAPPDATA "dev.codexlink.desktop"
if (Test-Path -LiteralPath $stateDir) {
    throw "refusing Desktop installer smoke because CodexLink user state already exists: $stateDir"
}
New-Item -ItemType Directory -Path $stateDir | Out-Null
$stateSentinel = Join-Path $stateDir ("installer-smoke-preserve-" + [guid]::NewGuid().ToString("N") + ".txt")
[System.IO.File]::WriteAllText($stateSentinel, "preserve-me`n", [System.Text.Encoding]::ASCII)

$installedDir = $null
$uninstaller = $null
$installed = $false
try {
    $installProcess = Start-Process -FilePath $Installer -ArgumentList @("/S", "/D=$InstallDir") -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "Desktop silent install failed with exit code $($installProcess.ExitCode)"
    }
    Wait-Until { $null -ne (Get-CodexLinkUninstallEntry) } 30 "Desktop installer did not register a current-user uninstall entry"
    $installed = $true

    $entry = Get-CodexLinkUninstallEntry
    if ([string]$entry.DisplayVersion -ne $ProductVersion) {
        throw "unexpected CodexLink product version in uninstall metadata: '$($entry.DisplayVersion)' (expected '$ProductVersion')"
    }
    $uninstaller = Resolve-UninstallExecutable ([string]$entry.UninstallString)
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "registered CodexLink uninstaller does not exist: $uninstaller"
    }
    $installedDir = if ($entry.InstallLocation) {
        Resolve-RegistryPath ([string]$entry.InstallLocation) "InstallLocation"
    } else {
        Split-Path -Parent ([System.IO.Path]::GetFullPath($uninstaller))
    }
    if (-not [string]::Equals(
        $installedDir.TrimEnd([System.IO.Path]::DirectorySeparatorChar),
        $InstallDir.TrimEnd([System.IO.Path]::DirectorySeparatorChar),
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "CodexLink installer did not honor the requested custom install path: $installedDir"
    }

    $desktopExe = Join-Path $installedDir "CodexLink.exe"
    if (-not (Test-Path -LiteralPath $desktopExe -PathType Leaf)) {
        throw "installed CodexLink executable is missing: $desktopExe"
    }
    $runtimeDir = Join-Path $installedDir "codexlink-core"
    if (-not (Test-Path -LiteralPath $runtimeDir -PathType Container)) {
        throw "installed CodexLink Core directory is missing: $runtimeDir"
    }

    $shortSource = $SourceSha.Substring(0, 12).ToLowerInvariant()
    $binaryMap = @(
        [pscustomobject]@{ File = "codexlink-core.exe"; Identity = "webcodex" },
        [pscustomobject]@{ File = "codexlink-core-server.exe"; Identity = "webcodex-server" },
        [pscustomobject]@{ File = "codexlink-core-runner.exe"; Identity = "webcodex-runner" }
    )
    foreach ($entry in $binaryMap) {
        $binary = Join-Path $runtimeDir $entry.File
        if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
            throw "installed CodexLink Core binary is missing: $binary"
        }
        $line = Get-VersionLine $binary ([System.IO.Path]::GetFileNameWithoutExtension($entry.File))
        $expected = "$($entry.Identity) $EngineVersion (commit $shortSource, dirty=false, built_at=$BuiltAt)"
        if ($line -ne $expected) {
            throw "unexpected installed $($entry.File) identity: '$line' (expected '$expected')"
        }
        Assert-BundledProductMode $binary ([System.IO.Path]::GetFileNameWithoutExtension($entry.File))
    }

    Assert-StandaloneCoreBlocked (Join-Path $runtimeDir "codexlink-core.exe")

    $legacyBrandPaths = @(
        Get-ChildItem -LiteralPath $installedDir -Recurse -Force -ErrorAction Stop |
            Where-Object { $_.Name -match '(?i)webcodex' }
    )
    if ($legacyBrandPaths.Count -ne 0) {
        throw "customer-visible install tree still contains legacy WebCodex path names: $($legacyBrandPaths.FullName -join ', ')"
    }

    $tunnelClient = Join-Path $runtimeDir "tools\tunnel-client.exe"
    if (-not (Test-Path -LiteralPath $tunnelClient -PathType Leaf)) {
        throw "installed bundled OpenAI tunnel-client is missing: $tunnelClient"
    }
    $expectedTunnelSha = "6649169733686805ca16cccd91774594d0c017fd729c37ad4ce1cd18323d9ae8"
    $tunnelSha = (Get-FileHash -LiteralPath $tunnelClient -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($tunnelSha -ne $expectedTunnelSha) {
        throw "unexpected installed OpenAI tunnel-client SHA-256: $tunnelSha"
    }

    $licenseText = Join-Path $runtimeDir "LICENSE-APACHE-2.0.txt"
    $licenseSource = Join-Path (Split-Path -Parent $PSScriptRoot) "LICENSE"
    if (-not (Test-Path -LiteralPath $licenseText -PathType Leaf)) {
        throw "installed WebCodex Apache-2.0 license text is missing"
    }
    $licenseSha = (Get-FileHash -LiteralPath $licenseText -Algorithm SHA256).Hash.ToLowerInvariant()
    $licenseSourceSha = (Get-FileHash -LiteralPath $licenseSource -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($licenseSha -ne $licenseSourceSha) {
        throw "installed WebCodex Apache-2.0 license text differs from source"
    }

    Write-Output "Desktop install smoke passed: $installedDir"
    Write-Output "Bundled CodexLink Core: $runtimeDir"
} finally {
    if ($installed) {
        if (-not $uninstaller) {
            $entry = Get-CodexLinkUninstallEntry
            if ($entry) { $uninstaller = Resolve-UninstallExecutable ([string]$entry.UninstallString) }
        }
        if ($uninstaller -and (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
            $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -PassThru
            if ($uninstallProcess.ExitCode -ne 0) {
                throw "Desktop silent uninstall failed with exit code $($uninstallProcess.ExitCode)"
            }
            Wait-Until { $null -eq (Get-CodexLinkUninstallEntry) } 30 "Desktop uninstall entry remained after silent uninstall"
            if ($installedDir) {
                $desktopExe = Join-Path $installedDir "CodexLink.exe"
                $runtimeDir = Join-Path $installedDir "codexlink-core"
                Wait-Until {
                    -not (Test-Path -LiteralPath $desktopExe) -and
                    -not (Test-Path -LiteralPath $runtimeDir) -and
                    -not (Test-Path -LiteralPath $uninstaller)
                } 30 "Desktop installer-owned files remained after silent uninstall: $installedDir"
            }
        }
    }
    if (-not (Test-Path -LiteralPath $stateSentinel -PathType Leaf)) {
        throw "CodexLink uninstall removed persistent user state: $stateSentinel"
    }
}

if (Test-Path -LiteralPath $stateSentinel -PathType Leaf) {
    Remove-Item -LiteralPath $stateSentinel -Force
}
if ((Test-Path -LiteralPath $stateDir -PathType Container) -and @(Get-ChildItem -LiteralPath $stateDir -Force).Count -eq 0) {
    Remove-Item -LiteralPath $stateDir -Force
}

Write-Output "Desktop uninstall smoke passed; CodexLink user state was preserved"
