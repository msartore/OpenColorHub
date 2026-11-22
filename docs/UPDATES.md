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

## Components are pinned, never updated in place

The app never updates PawnIO — or any other dependency — at runtime. There is no
component updater, no remote component manifest, and no "update available" button
for a dependency. The only thing that updates itself is the app, through the
signed `latest.json` above.

PawnIO is installed on demand only: when the driver is missing, the Fan screen
offers **Install PawnIO**, which downloads the versioned release pinned in
`crates/colorctl-core/src/pawnio_install.rs`, checks it against the pinned size
and SHA-256, and runs the vendor's own installer with its own window and UAC
prompt. Nothing is installed silently, and an already-installed driver is never
replaced by the app.

When a newer component version is needed, it ships **with a new release of this
app**: bump `VERSION`, `URL`, `SIZE` and `SHA256` together in `pawnio_install.rs`,
rebuild, test, tag and release. The app updater then delivers the new pin along
with the new app. A pinned file is never fetched from a `/latest/` URL, so a
release's bytes cannot change underneath it.

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
