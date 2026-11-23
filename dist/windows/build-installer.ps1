<#
.SYNOPSIS
  Build the Windows installer (omnix-setup-x64.exe).

.DESCRIPTION
  Stages omnix.exe, downloads the Microsoft WebView2 Evergreen Bootstrapper,
  and compiles dist/windows/omnix.iss with Inno Setup 6. The installer is
  per-user, so no admin prompt is needed, and it installs WebView2 only when
  the runtime is missing.
#>
[CmdletBinding()]
param(
    [string]$Binary = '',
    [string]$Version = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ScriptDir = $PSScriptRoot
$RootDir = Split-Path -Parent (Split-Path -Parent $ScriptDir)

if ([string]::IsNullOrEmpty($Binary)) {
    $Binary = Join-Path $RootDir 'target\release\omnix.exe'
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
        # cross-compiled from Linux: cargo xwin build --release --target x86_64-pc-windows-msvc
        $Binary = Join-Path $RootDir 'target\x86_64-pc-windows-msvc\release\omnix.exe'
    }
}
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "binary not found: $Binary (build it with: cargo build --release)"
}

if ([string]::IsNullOrEmpty($Version)) {
    $match = Select-String -Path (Join-Path $RootDir 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"'
    if (-not $match) { throw 'could not read the version from Cargo.toml' }
    $Version = $match.Matches[0].Groups[1].Value
}

$Stage = Join-Path $ScriptDir 'build'
New-Item -ItemType Directory -Path $Stage -Force | Out-Null
Copy-Item -LiteralPath $Binary -Destination (Join-Path $Stage 'omnix.exe') -Force

$Bootstrapper = Join-Path $ScriptDir 'MicrosoftEdgeWebView2Setup.exe'
if (-not (Test-Path -LiteralPath $Bootstrapper -PathType Leaf)) {
    Write-Host 'Downloading the WebView2 Evergreen Bootstrapper...'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' `
        -OutFile $Bootstrapper -UseBasicParsing
}
if (-not (Test-Path -LiteralPath $Bootstrapper -PathType Leaf)) {
    throw "WebView2 bootstrapper not found: $Bootstrapper"
}

function Find-Iscc {
    $found = @()
    foreach ($root in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        if ($root) {
            $found += Get-ChildItem -Path (Join-Path $root 'Inno Setup*\ISCC.exe') `
                -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
        }
    }
    if ($env:LOCALAPPDATA) {
        $found += Get-ChildItem -Path (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup*\ISCC.exe') `
            -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
    }
    return $found | Select-Object -First 1
}

$Iscc = Find-Iscc
if (-not $Iscc) {
    Write-Host 'Inno Setup not found; installing it with Chocolatey...'
    choco install innosetup --no-progress -y
    $Iscc = Find-Iscc
}
if (-not $Iscc) { throw 'ISCC.exe not found; install Inno Setup 6' }

$Out = Join-Path $Stage 'omnix-setup-x64.exe'
if (Test-Path -LiteralPath $Out) { Remove-Item -LiteralPath $Out -Force }

& $Iscc "/DMyAppVersion=$Version" "/O$Stage" (Join-Path $ScriptDir 'omnix.iss')
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
if (-not (Test-Path -LiteralPath $Out -PathType Leaf)) {
    throw "installer not produced: $Out"
}

Write-Host "Built $Out"
Write-Host 'The installer adds WebView2 automatically when it is missing.'
