# Packages the release binary as a portable build.
#
# A portable copy carries a `opencolorhub.portable` marker next to the executable;
# the app then keeps settings and state in `config/` and `data/` beside it
# instead of under %APPDATA%, so the whole folder can be moved or deleted.

param([string]$Version = "")

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target\release\opencolorhub.exe"
if (-not (Test-Path $exe)) {
  throw "release binary not found at $exe - run 'npx tauri build' first"
}

# Default the version to the executable's own FileVersion, so the package name
# always matches the binary it contains.
if (-not $Version) {
  $Version = (Get-Item $exe).VersionInfo.FileVersion
  if (-not $Version) { throw "cannot read a version from $exe" }
  $Version = $Version.Trim()
  Write-Host "version from the binary: $Version" -ForegroundColor DarkGray
}

$out = Join-Path $root "target\release\bundle\portable"
$pkg = Join-Path $out "OpenColorHub-$Version-portable-win64"
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
New-Item -ItemType Directory -Path $pkg -Force | Out-Null

Copy-Item $exe (Join-Path $pkg "OpenColorHub.exe")
Set-Content -Path (Join-Path $pkg "opencolorhub.portable") -Value "portable" -Encoding ascii

@"
OpenColorHub $Version - portable build
====================================

Run OpenColorHub.exe. Nothing is installed and no registry keys are written.

Settings and lighting state stay in this folder (config\ and data\), so the
whole directory can be moved to a USB stick or deleted without leaving
anything behind. Remove opencolorhub.portable to fall back to %APPDATA%.

Requires the Microsoft Edge WebView2 runtime, which ships with Windows 11 and
with current Windows 10 updates. If the window stays blank, install it from
https://developer.microsoft.com/microsoft-edge/webview2/

Fan telemetry on Windows is read-only and needs HWiNFO64 running with
"Shared Memory Support" enabled (Settings > Safety). Lighting works directly
over USB HID and needs no drivers.
"@ | Set-Content -Path (Join-Path $pkg "README.txt") -Encoding utf8

$zip = Join-Path $out "OpenColorHub-$Version-portable-win64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $pkg -DestinationPath $zip

$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "portable package: $zip ($size MB)"
