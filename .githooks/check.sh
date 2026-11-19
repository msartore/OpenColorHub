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

# Rust type-check the Linux target too. On a Windows host there is normally no
# x86_64-linux-gnu cross compiler; ring's build script only needs scratch tools
# for `cargo check` (which never links), so use the shims in stubs/. A real
# cross compiler, if configured via CC/AR, is always preferred. CI compiles
# for real on ubuntu-22.04.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    if [ -z "${CC_x86_64_unknown_linux_gnu:-}" ]; then
      stub_dir="$(cd "$(dirname "$0")/stubs" && pwd)"
      if command -v cygpath >/dev/null 2>&1; then
        CC_x86_64_unknown_linux_gnu="$(cygpath -w "$stub_dir/gcc.cmd")"
        AR_x86_64_unknown_linux_gnu="$(cygpath -w "$stub_dir/ar.cmd")"
      else
        CC_x86_64_unknown_linux_gnu="$stub_dir/gcc.cmd"
        AR_x86_64_unknown_linux_gnu="$stub_dir/ar.cmd"
      fi
      export CC_x86_64_unknown_linux_gnu AR_x86_64_unknown_linux_gnu
    fi
    ;;
esac

echo "[hooks] cargo check --target x86_64-unknown-linux-gnu"
cargo check -p colorctl-core --target x86_64-unknown-linux-gnu

echo "[hooks] all checks passed"
