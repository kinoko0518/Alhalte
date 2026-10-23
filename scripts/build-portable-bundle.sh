#!/usr/bin/env bash
#
# Builds a portable folder for the current Linux/macOS host:
#   dist/portable-<os>-<arch>/
#   dist/alhalte-portable-<os>-<arch>.tar.gz
#
# The folder contains alhalte-cafe (the one-shot launcher), its sibling
# services, the built SPA under web/, and Restaurant.toml. Run it through
# `nix run .#portable-bundle` or `nix develop -c` so cargo, pnpm, and the wasm
# toolchain are available.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CONFIG="Restaurant.portable.toml"
WEB_DIR="web/alhalte-argenterie"
PUBLIC_EXTENSIONS="$WEB_DIR/public/extensions"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
DIST="dist/portable-$OS-$ARCH"
ARCHIVE="dist/alhalte-portable-$OS-$ARCH.tar.gz"

export ALHALTE_ALLOW_PLAINTEXT_SECRETS=1

if [ ! -f "$WEB_DIR/public/wasm/alhalte_saignant.wasm" ]; then
  echo "error: $WEB_DIR/public/wasm/alhalte_saignant.wasm is missing." >&2
  echo "       Run 'nix run .#saignant-wasm' first." >&2
  exit 1
fi

echo "==> Building release binaries"
cargo build --release \
  -p alhalte-cafe \
  -p alhalte-maitre-d \
  -p alhalte-cellar \
  -p alhalte-banquette

echo "==> Building official WASM extensions"
for ext_manifest in extensions/*/Cargo.toml; do
  cargo build --manifest-path "$ext_manifest" --target wasm32-unknown-unknown --release
done

echo "==> Staging extensions (manifest + wasm)"
rm -rf "$PUBLIC_EXTENSIONS"
mkdir -p "$PUBLIC_EXTENSIONS"
target/release/alhalte-cafe --config "$CONFIG" stage-extensions \
  --output-dir "$PUBLIC_EXTENSIONS"

if [ ! -d "web/node_modules" ] || [ ! -d "$WEB_DIR/node_modules" ]; then
  echo "==> Installing web dependencies"
  (cd web && pnpm install)
fi

echo "==> Building production SPA"
pnpm --dir "$WEB_DIR" run build:app
if [ ! -f "$WEB_DIR/dist-app/index.html" ]; then
  echo "error: $WEB_DIR/dist-app/index.html was not produced." >&2
  exit 1
fi

echo "==> Assembling $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/web"
cp target/release/alhalte-cafe "$DIST/"
cp target/release/alhalte-maitre-d "$DIST/"
cp target/release/alhalte-cellar "$DIST/"
cp target/release/alhalte-banquette "$DIST/"
cp -a "$WEB_DIR/dist-app/." "$DIST/web/"
cp "$CONFIG" "$DIST/Restaurant.toml"
cp packaging/portable/README.txt "$DIST/README.txt"

echo "==> Creating $ARCHIVE"
mkdir -p dist
rm -f "$ARCHIVE"
tar -czf "$ARCHIVE" -C "$DIST" .

echo "Created $ROOT/$ARCHIVE"
