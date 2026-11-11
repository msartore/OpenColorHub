# Updates

On launch the app asks GitHub for the newest release. A higher tag offers the
update in a dialog; accepting downloads the release's `OpenColorHub.exe`, checks
it against `SHA256SUMS.txt` from the same release, swaps it in and restarts.
**Settings → Updates → Check now** forces a check. Nothing updates without the
dialog being accepted.

## How the swap works

Windows refuses to overwrite a running image but allows *renaming* it:

1. rename `OpenColorHub.exe` → `OpenColorHub.old.exe`
2. copy the downloaded build into place
3. restart through Tauri's own `restart()`
4. delete the leftover on the next launch

A failed copy rolls the rename back, so a broken update leaves a working app.
`restart()` rather than a manual spawn matters: the old process holds the WebView2
profile lock, and racing it produces a window that never appears.

## Why not the Tauri updater plugin

The plugin needs signing keys, a `latest.json` on every release and an installer
re-run. The release already publishes the bare executable, so the app fetches
that directly.

## What that costs

The download is verified against `SHA256SUMS.txt` over HTTPS, but these builds are
**not Authenticode signed**. Integrity rests on GitHub's TLS and that checksum,
not a vendor certificate. If that is not acceptable, sign the release binaries and
add the check to `update_ipc.rs`.

## Publishing a release

**The tag is the version.** Nothing is bumped by hand: the build job runs
`scripts/set-version.mjs` with the tag and rewrites `Cargo.toml`,
`tauri.conf.json`, `package.json` and the lock before compiling, so installer
metadata, runtime version and the updater's idea of "current" agree.

```bash
git tag -a v0.3.0 -m "OpenColorHub 0.3.0"
git push origin v0.3.0
```

A build tagged `v0.3.0` that still reports `0.2.0` makes the self-updater offer
the same release on every launch, forever.

The same script runs locally:

```bash
node scripts/set-version.mjs v0.3.0
pwsh scripts/build-release.ps1 -Tag v0.3.0 -Portable
```

Both platforms build, `dist/` is staged flat, and the release carries the NSIS
installer, MSI, portable zip, Linux packages, the bare `OpenColorHub.exe` the
updater downloads, and `SHA256SUMS.txt`. A separate job packages the CLI:
`opencolorhub-cli_<version>_amd64.deb` (binary, completions, udev rule and the
`/var/lib/colorctl` tmpfiles entry), a portable tarball, and a completions-only
tarball.
