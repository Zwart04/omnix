<#
.SYNOPSIS
  Package the release binary for Windows x64.

.DESCRIPTION
  Creates perch-windows-x64.zip containing perch.exe and a short README that
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
    $Binary = Join-Path $RootDir 'target\release\perch.exe'
}
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "binary not found: $Binary (build it with: cargo build --release)"
}

$OutDir = $ScriptDir
$Out = Join-Path $OutDir 'perch-windows-x64.zip'
$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ('perch-zip-' + [guid]::NewGuid().ToString('N'))

try {
    New-Item -ItemType Directory -Path $Stage -Force | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $Stage 'perch.exe')

    $Readme = @'
Perch for Windows x64

Run perch.exe with no arguments to open the app window.
Subcommands (history, agent, sleep, resume, handoff, browser) stay in the CLI.

Requirement: Microsoft Edge WebView2 Runtime must be installed.
Windows 11 ships it. On Windows 10 install it from:
https://developer.microsoft.com/microsoft-edge/webview2/

Perch stores state in %PERCH_HOME%\.perch or .perch under the working directory.
This build is unsigned, so SmartScreen may ask for confirmation.
'@
    Set-Content -LiteralPath (Join-Path $Stage 'README.txt') -Value $Readme -Encoding ASCII

    if (Test-Path -LiteralPath $Out) { Remove-Item -LiteralPath $Out -Force }
    Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Out -CompressionLevel Optimal

    Write-Host "Built $Out"
    Write-Host "Next: unpack it and run perch.exe"
}
finally {
    Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue
}
