param(
  [Parameter(Mandatory = $true)][string]$OutDir,
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$SkipInstaller
)

$ErrorActionPreference = "Stop"

$mt = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\mt.exe" -ErrorAction SilentlyContinue |
  Sort-Object FullName -Descending |
  Select-Object -First 1 -ExpandProperty FullName
if (-not $mt) { throw "mt.exe not found in the Windows SDK" }

$OutDir = (Resolve-Path $OutDir).Path
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifest = Join-Path $PSScriptRoot "app.manifest"
$appDir = Join-Path $OutDir "OpenColorHub"
$exe = Join-Path $appDir "OpenColorHub.exe"

# Ship the licence and attribution texts beside the executable. The archives
# and installer below are rebuilt after this copy, so they carry them too.
foreach ($file in @("LICENSE.md", "NOTICE.md", "crates/colorctl-core/resources/COPYING.LGPL-2.1")) {
  $source = Join-Path $repo $file
  if (Test-Path $source) { Copy-Item $source (Join-Path $appDir (Split-Path $file -Leaf)) -Force }
}

& $mt -manifest $manifest -outputresource:"$exe;#1"
if ($LASTEXITCODE -ne 0) { throw "mt.exe failed with exit code $LASTEXITCODE" }

$verify = Join-Path $env:TEMP "opencolorhub-manifest-verify.xml"
& $mt -inputresource:"$exe;#1" -out:$verify | Out-Null
if (-not (Select-String -Path $verify -Pattern "requireAdministrator" -Quiet)) {
  throw "the elevation manifest was not embedded"
}

# The zip and the update archive must carry the manifested executable too.
$plain = Join-Path $OutDir "OpenColorHub.zip"
$versioned = Join-Path $OutDir "OpenColorHub-$Version-windows-x86_64.zip"
Remove-Item $plain, $versioned -Force -ErrorAction SilentlyContinue
Compress-Archive -Path $appDir -DestinationPath $plain -CompressionLevel Optimal
Copy-Item $plain $versioned -Force

# The Inno installer embeds the same directory, so rebuild it as well.
if ($SkipInstaller) {
  Write-Host "Skipping the installer rebuild (local verification only)."
} else {
  $iscc = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
  ) | Where-Object { Test-Path $_ } | Select-Object -First 1
  if (-not $iscc) { throw "ISCC.exe not found" }
  $iss = Join-Path $OutDir "OpenColorHub.iss"
  New-Item -ItemType Directory -Force $OutDir | Out-Null
  & $iscc $iss /O"$OutDir" /Q
  if ($LASTEXITCODE -ne 0) { throw "ISCC.exe failed with exit code $LASTEXITCODE" }
}

Write-Host "Embedded the elevation manifest and rebuilt the Windows package artifacts."
