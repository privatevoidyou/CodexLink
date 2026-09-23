# Stage exact CodexLink Core bytes for a Windows x64 Tauri Desktop bundle.
#
# This helper consumes already-built CodexLink product-mode binaries, verifies
# one exact source/version/build identity plus fail-closed bundled admission, copies them into an ignored generated tree,
# proves the copies are byte-for-byte identical, and writes a Tauri config
# overlay that maps only those staged files into the customer-visible `codexlink-core` tree.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BinDir,
    [Parameter(Mandatory = $true)][string]$TunnelClient,
    [Parameter(Mandatory = $true)][string]$ProductVersion,
    [Parameter(Mandatory = $true)][string]$EngineVersion,
    [Parameter(Mandatory = $true)][string]$SourceSha,
    [Parameter(Mandatory = $true)][Int64]$BuiltAt,
    [Parameter(Mandatory = $true)][string]$OutputDir
)

$ErrorActionPreference = "Stop"

if ($ProductVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "invalid CodexLink product version '$ProductVersion'"
}
if ($EngineVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "invalid WebCodex Engine version '$EngineVersion'"
}
if ($SourceSha -notmatch '^[0-9A-Fa-f]{40}$') {
    throw "SourceSha must be one exact 40-hex Git commit"
}
if ($BuiltAt -le 0) {
    throw "BuiltAt must be a positive Unix timestamp"
}

$BinDir = [System.IO.Path]::GetFullPath($BinDir)
$TunnelClient = [System.IO.Path]::GetFullPath($TunnelClient)
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
if (Test-Path -LiteralPath $OutputDir) {
    throw "Desktop bundle output already exists: $OutputDir"
}
$tunnelClientVersion = "0.0.12"
$tunnelClientSha256 = "6649169733686805ca16cccd91774594d0c017fd729c37ad4ce1cd18323d9ae8"
if (-not (Test-Path -LiteralPath $TunnelClient -PathType Leaf)) {
    throw "missing OpenAI tunnel-client: $TunnelClient"
}
$tunnelClientItem = Get-Item -LiteralPath $TunnelClient
if (($tunnelClientItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "OpenAI tunnel-client must be a regular non-reparse file: $TunnelClient"
}
$tunnelClientHash = (Get-FileHash -LiteralPath $TunnelClient -Algorithm SHA256).Hash.ToLowerInvariant()
if ($tunnelClientHash -ne $tunnelClientSha256) {
    throw "unexpected OpenAI tunnel-client SHA-256: $tunnelClientHash"
}

$runtimeDir = Join-Path $OutputDir "resources\codexlink-core"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
$toolsDir = Join-Path $runtimeDir "tools"
New-Item -ItemType Directory -Path $toolsDir | Out-Null
$shortSource = $SourceSha.Substring(0, 12).ToLowerInvariant()
$binaryMap = @(
    [pscustomobject]@{ Source = "webcodex"; Destination = "codexlink-core" },
    [pscustomobject]@{ Source = "webcodex-server"; Destination = "codexlink-core-server" },
    [pscustomobject]@{ Source = "webcodex-runner"; Destination = "codexlink-core-runner" }
)
$resourceMap = [ordered]@{}
$fileMetadata = [ordered]@{}

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
        if (-not $process.Start()) { throw "$name.exe bundled admission verification could not start" }
        $null = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        if ($process.ExitCode -ne 78 -or -not $stderr.Contains("bundled CodexLink Core requires an activated CodexLink Desktop")) {
            throw "$name.exe is not a fail-closed CodexLink bundled product-mode binary"
        }
    } finally {
        $process.Dispose()
    }
}

try {
    foreach ($binary in $binaryMap) {
        $sourceName = $binary.Source
        $destinationName = $binary.Destination
        $source = Join-Path $BinDir "$sourceName.exe"
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "missing Desktop runtime binary: $source"
        }
        $sourceItem = Get-Item -LiteralPath $source
        if (($sourceItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Desktop runtime binary must be a regular non-reparse file: $source"
        }

        # Capture the native process to completion before selecting the first
        # output line. Piping the native process directly into Select-Object
        # -First can stop the upstream process early on PowerShell 7 and leave
        # a non-zero $LASTEXITCODE even though the version line was emitted.
        $output = @(& $source --version)
        $exitCode = $LASTEXITCODE
        $line = @($output | Select-Object -First 1)
        if ($exitCode -ne 0 -or $line.Count -eq 0 -or -not $line[0]) {
            throw "$sourceName.exe --version failed while staging Desktop resources (exit code $exitCode)"
        }
        $line = $line[0].TrimEnd()
        $expected = "$sourceName $EngineVersion (commit $shortSource, dirty=false, built_at=$BuiltAt)"
        if ($line -ne $expected) {
            throw "unexpected $sourceName.exe identity: '$line' (expected '$expected')"
        }

        Assert-BundledProductMode $source $sourceName

        $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        $destination = Join-Path $runtimeDir "$destinationName.exe"
        Copy-Item -LiteralPath $source -Destination $destination
        $destinationItem = Get-Item -LiteralPath $destination
        $destinationHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($destinationItem.Length -ne $sourceItem.Length -or $destinationHash -ne $sourceHash) {
            throw "staged CodexLink Core byte verification failed for $destinationName.exe"
        }

        $resourceMap[[System.IO.Path]::GetFullPath($destination)] = "codexlink-core/$destinationName.exe"
        $fileMetadata[$destinationName] = [ordered]@{
            filename = "$destinationName.exe"
            size = [Int64]$destinationItem.Length
            sha256 = $destinationHash
        }
    }

    $stagedTunnelClient = Join-Path $toolsDir "tunnel-client.exe"
    Copy-Item -LiteralPath $TunnelClient -Destination $stagedTunnelClient
    $stagedTunnelItem = Get-Item -LiteralPath $stagedTunnelClient
    $stagedTunnelHash = (Get-FileHash -LiteralPath $stagedTunnelClient -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($stagedTunnelItem.Length -ne $tunnelClientItem.Length -or $stagedTunnelHash -ne $tunnelClientSha256) {
        throw "staged OpenAI tunnel-client byte verification failed"
    }
    $resourceMap[[System.IO.Path]::GetFullPath($stagedTunnelClient)] = "codexlink-core/tools/tunnel-client.exe"
    $fileMetadata["tunnel-client"] = [ordered]@{
        filename = "tools/tunnel-client.exe"
        version = $tunnelClientVersion
        size = [Int64]$stagedTunnelItem.Length
        sha256 = $stagedTunnelHash
    }

    $licenseSource = Join-Path (Split-Path -Parent $PSScriptRoot) "LICENSE"
    if (-not (Test-Path -LiteralPath $licenseSource -PathType Leaf)) {
        throw "missing WebCodex Apache-2.0 license text: $licenseSource"
    }
    $licenseSourceItem = Get-Item -LiteralPath $licenseSource
    if (($licenseSourceItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "WebCodex license text must be a regular non-reparse file: $licenseSource"
    }
    $licenseSourceHash = (Get-FileHash -LiteralPath $licenseSource -Algorithm SHA256).Hash.ToLowerInvariant()
    $stagedLicense = Join-Path $runtimeDir "LICENSE-APACHE-2.0.txt"
    Copy-Item -LiteralPath $licenseSource -Destination $stagedLicense
    $stagedLicenseItem = Get-Item -LiteralPath $stagedLicense
    $stagedLicenseHash = (Get-FileHash -LiteralPath $stagedLicense -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($stagedLicenseItem.Length -ne $licenseSourceItem.Length -or $stagedLicenseHash -ne $licenseSourceHash) {
        throw "staged WebCodex license text byte verification failed"
    }
    $resourceMap[[System.IO.Path]::GetFullPath($stagedLicense)] = "codexlink-core/LICENSE-APACHE-2.0.txt"
    $fileMetadata["license"] = [ordered]@{
        filename = "LICENSE-APACHE-2.0.txt"
        size = [Int64]$stagedLicenseItem.Length
        sha256 = $stagedLicenseHash
    }

    $overlay = [ordered]@{
        version = $ProductVersion
        bundle = [ordered]@{
            active = $true
            targets = @("nsis")
            resources = $resourceMap
            windows = [ordered]@{
                nsis = [ordered]@{
                    installMode = "currentUser"
                }
            }
        }
    }
    $overlayPath = Join-Path $OutputDir "tauri.bundle.conf.json"
    $utf8 = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($overlayPath, ($overlay | ConvertTo-Json -Depth 8) + "`n", $utf8)

    $metadata = [ordered]@{
        schema_version = 2
        version = $ProductVersion
        product_version = $ProductVersion
        engine_version = $EngineVersion
        source_sha = $SourceSha.ToLowerInvariant()
        built_at = $BuiltAt
        product_mode = "verified-fail-closed-v2"
        resource_dir = "resources/codexlink-core"
        tunnel_client_required_for_codexlink = $true
        tunnel_client_sha256 = $tunnelClientSha256
        files = $fileMetadata
    }
    $metadataPath = Join-Path $OutputDir "desktop-bundle.json"
    [System.IO.File]::WriteAllText($metadataPath, ($metadata | ConvertTo-Json -Depth 8) + "`n", $utf8)

    Write-Output "CodexLink $ProductVersion staged with WebCodex Engine $EngineVersion from exact source $($SourceSha.ToLowerInvariant())"
    Write-Output "Tauri config overlay: $overlayPath"
    Write-Output "Runtime resources: $runtimeDir"
} catch {
    Write-Warning "Desktop bundle staging failed; preserving the new staging directory for inspection: $OutputDir"
    throw
}
