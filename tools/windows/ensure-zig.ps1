#!/usr/bin/env pwsh
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $ScriptDir)

$zigVersion = if ($env:ZIG_VERSION) { $env:ZIG_VERSION } else { "0.15.2" }
$zigBaseUrl = "https://ziglang.org/download/$zigVersion"
$toolchainDir = Join-Path $ProjectRoot ".tools/zig/toolchain"

$isWindows = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$isLinuxHost = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)
$isMacHost = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)

$exeName = if ($isWindows) { "zig.exe" } else { "zig" }
$zigBin = Join-Path $toolchainDir $exeName
if (Test-Path $zigBin) {
    Write-Output $zigBin
    exit 0
}

$arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
if ($isWindows) {
    switch ($arch) {
        "x64" { $pkg = "zig-x86_64-windows-$zigVersion.zip" }
        "arm64" { $pkg = "zig-aarch64-windows-$zigVersion.zip" }
        default { throw "unsupported Windows architecture: $arch" }
    }
} elseif ($isLinuxHost) {
    switch ($arch) {
        "x64" { $pkg = "zig-x86_64-linux-$zigVersion.tar.xz" }
        "arm64" { $pkg = "zig-aarch64-linux-$zigVersion.tar.xz" }
        default { throw "unsupported Linux architecture: $arch" }
    }
} elseif ($isMacHost) {
    switch ($arch) {
        "x64" { $pkg = "zig-x86_64-macos-$zigVersion.tar.xz" }
        "arm64" { $pkg = "zig-aarch64-macos-$zigVersion.tar.xz" }
        default { throw "unsupported macOS architecture: $arch" }
    }
} else {
    throw "unsupported operating system"
}

$zigRoot = Join-Path $ProjectRoot ".tools/zig"
New-Item -ItemType Directory -Path $zigRoot -Force | Out-Null

$tmpDir = Join-Path $zigRoot (".tmp-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmpDir | Out-Null

try {
    $archivePath = Join-Path $tmpDir $pkg
    $url = "$zigBaseUrl/$pkg"
    Write-Host "==> Downloading Zig $zigVersion"
    Invoke-WebRequest -Uri $url -OutFile $archivePath

    Write-Host "==> Installing Zig into $toolchainDir"
    if ($pkg.EndsWith(".zip")) {
        Expand-Archive -LiteralPath $archivePath -DestinationPath (Join-Path $tmpDir "extract")
        $extractBase = Join-Path $tmpDir "extract"
    } else {
        tar -xf $archivePath -C $tmpDir
        $extractBase = $tmpDir
    }

    $extracted = Get-ChildItem -Path $extractBase -Directory | Where-Object { $_.Name -like "zig-*" } | Select-Object -First 1
    if (-not $extracted) {
        throw "unable to locate extracted Zig toolchain contents"
    }

    if (Test-Path $toolchainDir) {
        Remove-Item -Recurse -Force $toolchainDir
    }
    Move-Item -Path $extracted.FullName -Destination $toolchainDir
} finally {
    if (Test-Path $tmpDir) {
        Remove-Item -Recurse -Force $tmpDir
    }
}

if (-not (Test-Path $zigBin)) {
    throw "Zig executable not found at $zigBin"
}

Write-Output $zigBin
