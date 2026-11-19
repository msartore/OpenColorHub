#!/bin/sh
# Shared verification for pre-commit and pre-push.
# Keep this the single source of truth so both hooks enforce the same checks.
set -e

export PATH="$HOME/.cargo/bin:$PATH"

echo "[hooks] cargo fmt --check"
cargo fmt --all -- --check

echo "[hooks] cargo clippy -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo "[hooks] cargo test"
cargo test --workspace

echo "[hooks] frontend build"
npm run build

echo "[hooks] all checks passed"
