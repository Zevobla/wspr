#Requires -Version 7
#
# bundle-windows.ps1 -- assemble a portable Windows zip for whspr-app
# (whspr-app.exe + the native ML DLLs it load-time links). Runnable both
# locally and from CI (see .github/workflows/release.yml).
#
# What it does, and why:
#
#   * Builds the release binary with `cargo build -p whspr-app --release
#     --target <Target>`. Unlike the macOS bundler this does NOT go through
#     Nix: the Windows build provisions a native MSVC toolchain, libclang and
#     the pinned Archivo faces itself (see .github/workflows/windows.yml), and
#     this script assumes the caller already did so -- it only re-asserts that
#     ARCHIVO_DIR is set, because crates/whspr-app/build.rs panics without it.
#
#   * Stages whspr-app.exe together with the FOUR sherpa/onnxruntime shared
#     libraries it load-time links. Windows has no rpath, so the exe will NOT
#     start unless these DLLs sit BESIDE it; the sherpa-rs-sys build script
#     drops them into the target profile dir, and we copy them next to the exe
#     in the zip. A missing exe or DLL is a hard error -- a silent partial
#     bundle would be a shipping bug.
#
#   * Produces a distributable portable zip: whspr-<version>-<arch>.zip. There
#     is no installer here -- the MSI is built separately from
#     packaging/windows/whspr.wxs. UNSIGNED by default; Authenticode signing
#     is layered on in CI when the WINDOWS_CERTIFICATE secret is present (see
#     release.yml).
#
# No build artifact produced here (.exe, .dll, .zip) is ever committed;
# everything lands under the output dir (default: dist/), which .gitignore
# excludes.

[CmdletBinding()]
param(
    # Rust target triple. Defaults to x64; aarch64-pc-windows-msvc is accepted
    # for a later arm64 build.
    [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')]
    [string]$Target = 'x86_64-pc-windows-msvc',

    # Cargo profile. `release` (default) maps to `--release` and the release
    # profile dir; `debug` builds unoptimized.
    [ValidateSet('release', 'debug')]
    [string]$Configuration = 'release',

    # App version (default: read from Cargo.toml). A leading "v" (e.g. from a
    # git tag v0.1.0) is stripped.
    [string]$Version = '',

    # Output directory (default: <repo>/dist).
    [string]$OutDir = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# --- Windows-only guard ----------------------------------------------------
# This script bundles a Windows .exe + DLLs; the MSVC target and the
# DLL-adjacency layout only make sense there.
if (-not $IsWindows) {
    throw 'this script bundles a Windows zip and must run on Windows'
}

$ScriptDir = Split-Path -Parent $PSCommandPath
$RepoRoot  = Split-Path -Parent $ScriptDir

# --- version ---------------------------------------------------------------
# Default to the workspace version in Cargo.toml ([workspace.package]); the
# whspr-app crate inherits it via `version.workspace = true`.
if ([string]::IsNullOrWhiteSpace($Version)) {
    $cargoToml = Join-Path $RepoRoot 'Cargo.toml'
    $match = Select-String -LiteralPath $cargoToml -Pattern '^version = "([^"]+)"' |
        Select-Object -First 1
    if ($null -eq $match) {
        throw "could not determine version from $cargoToml"
    }
    $Version = $match.Matches[0].Groups[1].Value
}
# Tags are `vX.Y.Z`; the artifact name wants a bare X.Y.Z.
$Version = $Version -replace '^v', ''
if ([string]::IsNullOrWhiteSpace($Version)) {
    throw 'could not determine version'
}

# --- friendly arch label ---------------------------------------------------
# x86_64-pc-windows-msvc -> x64, aarch64-pc-windows-msvc -> arm64.
$Arch = switch -Wildcard ($Target) {
    'x86_64-*'  { 'x64' }
    'aarch64-*' { 'arm64' }
    default     { throw "unsupported target triple: $Target" }
}

Write-Host "==> whspr Windows bundle: version $Version ($Arch, $Target, $Configuration)"

# --- Archivo faces guard ---------------------------------------------------
# crates/whspr-app/build.rs panics unless ARCHIVO_DIR holds the pinned Archivo
# faces (there is no build.rs network fetch). Unlike the macOS bundler we do
# NOT resolve it from Nix here -- the Windows build provisions it (see
# .github/workflows/windows.yml); we only assert the caller set it.
if ([string]::IsNullOrWhiteSpace($env:ARCHIVO_DIR)) {
    throw 'ARCHIVO_DIR is not set; crates/whspr-app/build.rs needs the pinned Archivo faces to compile. Provision them first (see .github/workflows/windows.yml).'
}
if (-not (Test-Path -LiteralPath $env:ARCHIVO_DIR -PathType Container)) {
    throw "ARCHIVO_DIR ('$env:ARCHIVO_DIR') is not a directory"
}
Write-Host "==> Archivo faces: $env:ARCHIVO_DIR"

# --- build the binary ------------------------------------------------------
$cargoArgs = @('build', '-p', 'whspr-app', '--target', $Target)
if ($Configuration -eq 'release') {
    $cargoArgs += '--release'
}
Write-Host "==> building: cargo $($cargoArgs -join ' ')"
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed (exit $LASTEXITCODE)"
}

# The sherpa-rs-sys build script drops the ML DLLs into this same profile dir.
$ProfileDir = Join-Path $RepoRoot (Join-Path 'target' (Join-Path $Target $Configuration))
Write-Host "==> built into: $ProfileDir"

# --- staging tree ----------------------------------------------------------
# Everything the portable zip ships lives under a single top-level "whspr"
# folder, so the archive extracts to whspr\whspr-app.exe (with its DLLs beside
# it), mirroring the macOS bundler's --keepParent layout.
if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $RepoRoot 'dist'
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$StageDir = Join-Path $OutDir 'whspr'
if (Test-Path -LiteralPath $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null

# The executable.
$ExeName = 'whspr-app.exe'
$ExeSrc  = Join-Path $ProfileDir $ExeName
if (-not (Test-Path -LiteralPath $ExeSrc -PathType Leaf)) {
    throw "expected binary not found at $ExeSrc"
}
Copy-Item -LiteralPath $ExeSrc -Destination (Join-Path $StageDir $ExeName)
Write-Host "==> staged $ExeName"

# --- DLL adjacency (load-time-linked ML libs) ------------------------------
# whspr-app load-time links these sherpa/onnxruntime SHARED libraries. Windows
# has no rpath, so whspr-app.exe will NOT start unless all four sit BESIDE it.
# The sherpa-rs-sys build script drops them into the profile dir on a real
# build; we copy every one next to the exe. A missing DLL is a hard error --
# shipping a partial bundle would crash the app on launch before a window ever
# shows, and the headless build gate never launches the GUI to catch it.
$Dlls = @(
    'onnxruntime.dll',
    'onnxruntime_providers_shared.dll',
    'sherpa-onnx-c-api.dll',
    'sherpa-onnx-cxx-api.dll'
)
foreach ($dll in $Dlls) {
    $dllSrc = Join-Path $ProfileDir $dll
    if (-not (Test-Path -LiteralPath $dllSrc -PathType Leaf)) {
        throw "required DLL '$dll' not found at $dllSrc; refusing to ship a broken bundle (the sherpa-rs-sys build script drops it into the target profile dir on a real build)."
    }
    Copy-Item -LiteralPath $dllSrc -Destination (Join-Path $StageDir $dll)
    Write-Host "    + $dll"
}

# --- portable zip ----------------------------------------------------------
$Zip = Join-Path $OutDir "whspr-$Version-$Arch.zip"
if (Test-Path -LiteralPath $Zip) {
    Remove-Item -LiteralPath $Zip -Force
}
Write-Host "==> zipping -> $Zip"
Compress-Archive -Path $StageDir -DestinationPath $Zip -CompressionLevel Optimal

Write-Host ''
Write-Host '==> done. Outputs:'
Write-Host "      staging: $StageDir"
Write-Host "      zip:     $Zip"
