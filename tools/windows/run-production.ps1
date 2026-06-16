#!/usr/bin/env pwsh
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $ScriptDir)
$cargoToml = Get-Content -Raw -Path (Join-Path $ProjectRoot "Cargo.toml")
$nameMatch = [regex]::Match($cargoToml, '(?m)^\s*name\s*=\s*"([^"]+)"')
if (-not $nameMatch.Success) {
    throw "unable to read binary name from Cargo.toml"
}
$BinName = $nameMatch.Groups[1].Value
$AppDir = Join-Path $ProjectRoot "dist/$BinName"
$Runner = Join-Path $AppDir "run.sh"
$BinPath = Join-Path $AppDir "$BinName.exe"

$isLinuxHost = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)
$isMacHost = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)
if ($isLinuxHost -or $isMacHost) {
    $BinPath = Join-Path $AppDir $BinName
}

if (Test-Path $Runner) {
    & $Runner @args
    exit $LASTEXITCODE
}

if (-not (Test-Path $BinPath)) {
    throw "binary not found at $BinPath. Build first with .\tools\windows\build-production.ps1 (Windows) or ./tools/linux/build-production.sh (macOS/Linux)."
}

& $BinPath @args
exit $LASTEXITCODE
