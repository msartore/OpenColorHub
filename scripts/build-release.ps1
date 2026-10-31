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
    [string]$Bundles = "all",
    [switch]$Portable,
    # Release version for a local build, e.g. `-Tag v0.3.0`. The release
    # workflow derives this from the git tag; locally it is opt-in.
    [string]$Tag = ""
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    if ($Tag) {
        node scripts/set-version.mjs $Tag
        if ($LASTEXITCODE -ne 0) { throw "set-version rejected '$Tag'" }
    }

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

    # The executable must be newer than every input it claims to contain;
    # otherwise a build that did not relink leaves the previous binary to be
    # packaged. Comparing against the newest source rather than against the
    # build start time keeps this correct when cargo legitimately skips work.
    $exe = Join-Path $repo "target\release\opencolorhub.exe"
    if (-not (Test-Path $exe)) { throw "no executable at $exe" }
    $stamp = (Get-Item $exe).LastWriteTime
    $newest = Get-ChildItem -Path $repo -Recurse -File -Include *.rs, *.ts, *.css, *.html, *.json, *.toml |
        Where-Object { $_.FullName -notmatch '\\target\\|\\node_modules\\|\\dist\\|\\gen\\' } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($newest -and $stamp -lt $newest.LastWriteTime) {
        throw "executable ($stamp) is older than $($newest.Name) ($($newest.LastWriteTime)) - refusing to package a stale binary"
    }

    if ($Portable) {
        & (Join-Path $PSScriptRoot "portable.ps1")
        if ($LASTEXITCODE -ne 0) { throw "portable packaging failed ($LASTEXITCODE)" }
    }
}
finally {
    Pop-Location
}
