#!/usr/bin/env bash
#
# Builds the portable Windows bundle:
#   dist/alhalte-windows-x86_64.zip
#
# Expected to run inside `nix develop` or via `nix run .#windows-bundle`, which
# provide the Windows rust-std, the mingw-w64 cross compiler, Node/pnpm, and zip.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="x86_64-pc-windows-gnu"
CONFIG="Restaurant.portable.toml"
WEB_DIR="web/alhalte-argenterie"
PUBLIC_EXTENSIONS="$WEB_DIR/public/extensions"
DIST="dist/windows"
ZIP="dist/alhalte-windows-x86_64.zip"

if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  echo "error: x86_64-w64-mingw32-gcc was not found." >&2
  echo "       Run this through 'nix run .#windows-bundle' or 'nix develop -c'." >&2
  exit 1
fi

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-$(command -v x86_64-w64-mingw32-gcc)}"
export CC_x86_64_pc_windows_gnu="${CC_x86_64_pc_windows_gnu:-$(command -v x86_64-w64-mingw32-gcc)}"
export AR_x86_64_pc_windows_gnu="${AR_x86_64_pc_windows_gnu:-$(command -v x86_64-w64-mingw32-ar)}"
export CXX_x86_64_pc_windows_gnu="${CXX_x86_64_pc_windows_gnu:-$(command -v x86_64-w64-mingw32-g++)}"
# The portable configuration uses plaintext local-only secrets. The default
# stack is fail-closed, so opt in explicitly for this portable build.
export ALHALTE_ALLOW_PLAINTEXT_SECRETS=1

if [ ! -f "$WEB_DIR/public/wasm/alhalte_saignant.wasm" ]; then
  echo "error: $WEB_DIR/public/wasm/alhalte_saignant.wasm is missing." >&2
  echo "       Run 'nix run .#saignant-wasm' first." >&2
  exit 1
fi

echo "==> Building host alhalte-cafe (for extension staging)"
cargo build -p alhalte-cafe

echo "==> Building official WASM extensions"
for ext_manifest in extensions/*/Cargo.toml; do
  cargo build --manifest-path "$ext_manifest" --target wasm32-unknown-unknown --release
done

echo "==> Staging extensions (manifest + wasm)"
rm -rf "$PUBLIC_EXTENSIONS"
mkdir -p "$PUBLIC_EXTENSIONS"
target/debug/alhalte-cafe --config "$CONFIG" stage-extensions \
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

echo "==> Cross-building Windows binaries"
# The nixpkgs mingw-w64 runtime does not ship libpthread.a, but the Rust
# x86_64-pc-windows-gnu target spec passes `-l:libpthread.a` to the linker.
# The Windows std does not reference pthread symbols, so an empty archive is
# enough to satisfy the linker. Add its directory to the linker search path for
# this target only.
STUB_LIB="$ROOT/target/windows-link-lib"
mkdir -p "$STUB_LIB"
x86_64-w64-mingw32-ar rcs "$STUB_LIB/libpthread.a"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=-L$STUB_LIB"

cargo build --release --target "$TARGET" \
  -p alhalte-cafe \
  -p alhalte-maitre-d \
  -p alhalte-cellar \
  -p alhalte-banquette

echo "==> Assembling $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/web"
cp "target/$TARGET/release/alhalte-cafe.exe" "$DIST/"
cp "target/$TARGET/release/alhalte-maitre-d.exe" "$DIST/"
cp "target/$TARGET/release/alhalte-cellar.exe" "$DIST/"
cp "target/$TARGET/release/alhalte-banquette.exe" "$DIST/"
cp -a "$WEB_DIR/dist-app/." "$DIST/web/"
cp "$CONFIG" "$DIST/Restaurant.toml"
cp packaging/portable/README.txt "$DIST/README.txt"

echo "==> Creating $ZIP"
mkdir -p dist
rm -f "$ZIP"
(cd "$DIST" && zip -qr "../$(basename "$ZIP")" .)

echo "Created $ROOT/$ZIP"
