# Git hooks

`pre-commit` and `pre-push` both run `.githooks/check.sh`:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `npm run build`
5. `cargo check -p colorctl-core --target x86_64-unknown-linux-gnu`

The Linux-target check catches errors in `#[cfg(target_os = "linux")]` code that
a Windows host build never compiles. On Windows the `stubs/` batch files stand
in for a cross compiler: `cargo check` never links, and ring's build script only
needs scratch tools. A real cross compiler configured through
`CC_x86_64_unknown_linux_gnu` / `AR_x86_64_unknown_linux_gnu` takes precedence.
CI still compiles and links for real on `ubuntu-22.04`.

The hooks are versioned here and enabled with:

```bash
git config core.hooksPath .githooks
```

or `npm run hooks:install`. The `pre-push` hook repeats the checks so a commit
made with `git commit --no-verify` is still caught before it leaves the machine.

These are client-side hooks: `--no-verify` on `git push` can still skip them.
Only server-side CI and required status checks can make that impossible.
