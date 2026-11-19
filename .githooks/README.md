# Git hooks

`pre-commit` and `pre-push` both run `.githooks/check.sh`:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `npm run build`

The hooks are versioned here and enabled with:

```bash
git config core.hooksPath .githooks
```

or `npm run hooks:install`. The `pre-push` hook repeats the checks so a commit
made with `git commit --no-verify` is still caught before it leaves the machine.

These are client-side hooks: `--no-verify` on `git push` can still skip them.
Only server-side CI and required status checks can make that impossible.
