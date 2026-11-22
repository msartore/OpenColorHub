# Updates

The app updates itself through Native Shell's updater. Settings → Updates calls
`checkForUpdate()`; a newer release shows its version, notes and a **Download &
install** action. `downloadUpdate()` reports bytes, then `installAndRestart()`
replaces the app and restarts it. Nothing updates without the user pressing the
button.

## Source and manifest

`nsh/nsh.json` names the source:

```json
"updater": { "github": "msartore/OpenColorHub" }
```

The release workflow builds the Windows zip and Linux tarball, then runs:

```bash
nsh update-manifest *-windows-x86_64.zip *-linux-x86_64.tar.gz \
  --entry nsh \
  --base-url "https://github.com/msartore/OpenColorHub/releases/download/vX.Y.Z/" \
  -o pkg
```

`latest.json` carries the app id, version and one archive per platform with
size and SHA-256. It is published beside the archives. NSH verifies the
manifest, the app id, the version (only newer is offered), the HTTPS URL and the
archive's hash before replacing anything.

A development run (`nsh run`) returns `null` from the check, so the panel says
updates need a packaged build.

## Publishing a release

**The tag is the version.** `scripts/set-version.mjs` writes it into
`Cargo.toml` (core + CLI), `nsh/nsh.json` and `nsh/Cargo.toml` before compiling,
so the installer, runtime and manifest agree.

```bash
git tag -a v0.3.0 -m "OpenColorHub 0.3.0"
git push origin v0.3.0
```

The workflow builds the NSH app on Windows and Linux, the CLI `.deb` and
tarballs, writes `latest.json`, and publishes one release. A partial platform
failure fails the release: there is no Windows-only NSH release followed by a
Linux one.

## Integrity

Builds are **not Authenticode signed**. Integrity rests on GitHub's TLS, NSH's
manifest and archive hashes, and the checksums in `SHA256SUMS.txt`. If that is
not acceptable, sign the Windows executables before packaging; NSH does not
change the operating system's publisher warning.

## Existing Tauri installs

The previous shell's updater downloads a bare `OpenColorHub.exe` and renames it
into place. A Native Shell app is a directory with its runtime, so that updater
cannot move an installation onto it. Existing users install this release once by
hand; from then on `latest.json` drives updates. The release notes say so.
