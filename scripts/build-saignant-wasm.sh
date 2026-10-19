#!/usr/bin/env bash
#
# Builds `alhalte-saignant` for `wasm32-unknown-unknown` and stages the artifact
# at `web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm`.
#
# Preferred entry points (they provide an unwrapped clang, llvm-ar and wasm-ld):
#
#   nix run .#saignant-wasm
#   nix develop -c ./scripts/build-saignant-wasm.sh
#
# Direct usage is also supported when CC_wasm32_unknown_unknown /
# AR_wasm32_unknown_unknown point at an unwrapped clang / llvm-ar and `wasm-ld`
# is on PATH (Issue #163).
#
# Reproducibility: paths are remapped so the same toolchain + crate sources
# produce the same bytes regardless of the checkout / Cargo directory, and the
# build is non-incremental. The committed artifact is checked against the Rust
# ABI version by `crates/alhalte-saignant/tests/artifact_test.rs`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v wasm-ld >/dev/null 2>&1; then
  echo "error: wasm-ld not found on PATH." >&2
  echo "       Run this script through 'nix run .#saignant-wasm' or 'nix develop -c'." >&2
  exit 1
fi

# The zstd C sources bundled by tin must be compiled for the bare wasm target,
# not for WASI. The devShell sets WASI flags for the extension builds, so set the
# target explicitly here.
export CC_wasm32_unknown_unknown="${CC_wasm32_unknown_unknown:-clang}"
export AR_wasm32_unknown_unknown="${AR_wasm32_unknown_unknown:-llvm-ar}"
export CFLAGS_wasm32_unknown_unknown="--target=wasm32-unknown-unknown"
export CARGO_INCREMENTAL=0

SYSROOT="$(rustc --print sysroot)"
CARGO_DIR="${CARGO_HOME:-$HOME/.cargo}"
export RUSTFLAGS="${RUSTFLAGS:-} \
  --remap-path-prefix=${ROOT}=/alhalte \
  --remap-path-prefix=${SYSROOT}=/rust \
  --remap-path-prefix=${CARGO_DIR}=/cargo"

echo "Building alhalte-saignant for wasm32-unknown-unknown (release)..."
cargo build -p alhalte-saignant --target wasm32-unknown-unknown --release

ARTIFACT="target/wasm32-unknown-unknown/release/alhalte_saignant.wasm"
DEST="web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm"
install -m 0644 "$ARTIFACT" "$DEST"

echo "Staged $DEST"
