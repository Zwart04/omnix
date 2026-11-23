<#
.SYNOPSIS
  Package the release binary for Windows x64.

.DESCRIPTION
  Creates omnix-windows-x64.zip containing omnix.exe and a short README that
  states the WebView2 runtime requirement. Requires PowerShell 5 or newer.
#>
[CmdletBinding()]
param(
    [string]$Binary = ''
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

$OutDir = $ScriptDir
$Out = Join-Path $OutDir 'omnix-windows-x64.zip'
$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ('omnix-zip-' + [guid]::NewGuid().ToString('N'))

try {
    New-Item -ItemType Directory -Path $Stage -Force | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $Stage 'omnix.exe')
    Copy-Item -LiteralPath (Join-Path $RootDir 'LICENSE') -Destination (Join-Path $Stage 'LICENSE.txt')

    $Readme = @'
Omnix for Windows x64

Run omnix.exe with no arguments to open the app window.
Subcommands (history, agent, sleep, resume, handoff, browser) stay in the CLI.

Requirement: Microsoft Edge WebView2 Runtime must be installed.
Windows 11 ships it. On Windows 10 install it from:
https://developer.microsoft.com/microsoft-edge/webview2/

Omnix stores state in %OMNIX_HOME%\.omnix or .omnix under the working directory.
This build is unsigned, so SmartScreen may ask for confirmation.
'@
    Set-Content -LiteralPath (Join-Path $Stage 'README.txt') -Value $Readme -Encoding ASCII

    if (Test-Path -LiteralPath $Out) { Remove-Item -LiteralPath $Out -Force }
    Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Out -CompressionLevel Optimal

    Write-Host "Built $Out"
    Write-Host "Next: unpack it and run omnix.exe"
}
finally {
    Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue
}
