<#
.SYNOPSIS
    Build the release bundles without embedding this machine's paths.

.DESCRIPTION
    Rust bakes absolute source locations into panic messages and `file!()`
    strings, so a stock `npx tauri build` produces an executable containing
    `C:\Users\<you>\...` and the checkout path. `cargo`'s `trim-paths` option
    that would fix this is still unstable (Cargo 1.99), so the supported route
    is `--remap-path-prefix`.

    The prefixes are derived from the environment at build time rather than
    written into the repository, so no home directory is committed anywhere.

.EXAMPLE
    pwsh scripts/build-release.ps1
    pwsh scripts/build-release.ps1 -Bundles nsis
#>
[CmdletBinding()]
param(
    [string]$Bundles = "all"
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    $remaps = @(
        "--remap-path-prefix=$env:USERPROFILE=~"
        "--remap-path-prefix=$repo=."
    )
    if ($env:CARGO_HOME) {
        $remaps += "--remap-path-prefix=$env:CARGO_HOME=~/.cargo"
    }

    $env:RUSTFLAGS = ($remaps -join " ")
    Write-Host "RUSTFLAGS = $env:RUSTFLAGS" -ForegroundColor DarkGray

    if ($Bundles -eq "all") {
        npx tauri build
    } else {
        npx tauri build --bundles $Bundles
    }
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
}
finally {
    Pop-Location
}
