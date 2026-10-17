{
  description = "Alhalte - self-hosted VTT module suite";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        fx = fenix.packages.${system};
        toolchain = fx.combine [
          fx.stable.cargo
          fx.stable.rustc
          fx.stable.clippy
          fx.stable.rustfmt
          fx.stable.rust-src
          fx.stable.rust-analyzer
          fx.targets.wasm32-unknown-unknown.stable.rust-std
          fx.targets.wasm32-wasip1.stable.rust-std
        ];
        llvm = pkgs.llvmPackages;
        wasiSysroot = pkgs.symlinkJoin {
          name = "wasi-sysroot";
          paths = [ pkgs.pkgsCross.wasi32.wasilibc pkgs.pkgsCross.wasi32.wasilibc.dev ];
        };
        wasiCflags = "--target=wasm32-wasip1 --sysroot=${wasiSysroot} -isystem ${wasiSysroot}/include/wasm32-wasip1";

        bistrotRunner = pkgs.writeShellApplication {
          name = "bistrot-runner";
          runtimeInputs = [
            toolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.sqlite
            pkgs.nodejs_22
            pkgs.pnpm
            pkgs.curl
            pkgs.xdg-utils
            pkgs.coreutils
          ];
          text = ''
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.sqlite.dev}/lib/pkgconfig:''${PKG_CONFIG_PATH:-}"

            CONFIG="./Bistrot.toml"
            NO_BROWSER=false

            while [ $# -gt 0 ]; do
              case "$1" in
                --config)
                  CONFIG="$2"
                  shift 2
                  ;;
                --no-browser)
                  NO_BROWSER=true
                  shift
                  ;;
                *)
                  echo "Unknown option: $1" >&2
                  exit 1
                  ;;
              esac
            done

            if [ ! -f "$CONFIG" ]; then
              echo "Error: Configuration file '$CONFIG' not found." >&2
              exit 1
            fi

            # 2. 必要なデータ・ソケットディレクトリを作成
            mkdir -p .alhalte/data/cellar/tin .alhalte/sockets

            # 3. Rust バイナリをビルドし、web の依存確認を行う
            echo "Building Rust workspace binaries..."
            cargo build --bins

            echo "Building official WASM extensions..."
            for ext_manifest in extensions/*/Cargo.toml; do
              cargo build --manifest-path "$ext_manifest" --target wasm32-unknown-unknown --release
            done

            target/debug/alhalte-bistrot --config "$CONFIG" stage-extensions \
              --output-dir "$PWD/web/alhalte-argenterie/public/extensions"

            if [ ! -d "web/node_modules" ] || [ ! -d "web/alhalte-argenterie/node_modules" ]; then
              echo "Installing web workspace dependencies..."
              (cd web && pnpm install)
            fi

            # 4. systemd unit ファイル群を $XDG_RUNTIME_DIR/systemd/user に出力
            RUNTIME_DIR="''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
            UNIT_DIR="''${RUNTIME_DIR}/systemd/user"
            mkdir -p "$UNIT_DIR"

            echo "Generating systemd user units into $UNIT_DIR..."
            target/debug/alhalte-bistrot --config "$CONFIG" systemd \
              --bin-dir "$PWD/target/debug" \
              --working-dir "$PWD" \
              --web-dir "$PWD/web/alhalte-argenterie" \
              --output-dir "$UNIT_DIR"

            # 5. systemctl --user daemon-reload & restart
            if command -v systemctl >/dev/null 2>&1 && systemctl --user import-environment PATH 2>/dev/null && systemctl --user daemon-reload 2>/dev/null && systemctl --user restart alhalte-bistrot.target 2>/dev/null; then
              echo "Successfully started alhalte-bistrot.target via systemd --user."
            else
              echo "Notice: systemd --user bus is not accessible; falling back to direct background processes..." >&2
              read -r -a maitre_args <<< "$(target/debug/alhalte-bistrot --config "$CONFIG" args maitre-d)"
              target/debug/alhalte-maitre-d "''${maitre_args[@]}" &
              read -r -a cellar_args <<< "$(target/debug/alhalte-bistrot --config "$CONFIG" args cellar)"
              target/debug/alhalte-cellar "''${cellar_args[@]}" &
              (cd "$PWD/web/alhalte-argenterie" && pnpm run dev) &
            fi

            # 6. argenterie の URL を取得し、ポート待受を最大15秒待機後、ブラウザを開く
            URL="$(target/debug/alhalte-bistrot --config "$CONFIG" url)"
            echo "Waiting for Argenterie frontend at $URL (up to 15s)..."

            READY=false
            for _ in $(seq 1 30); do
              if curl -s -f -o /dev/null "$URL" 2>/dev/null; then
                READY=true
                break
              fi
              sleep 0.5
            done

            if [ "$READY" = true ]; then
              echo "Alhalte Argenterie is running and ready at $URL"
            else
              echo "Warning: Argenterie did not respond within 15 seconds at $URL" >&2
            fi

            if [ -z "''${ALHALTE_NO_BROWSER:-}" ] && [ "$NO_BROWSER" = false ]; then
              echo "Opening browser: $URL"
              if [ -n "''${BROWSER:-}" ]; then
                "''${BROWSER}" "$URL" 2>/dev/null || true
              elif command -v xdg-open >/dev/null 2>&1; then
                xdg-open "$URL" 2>/dev/null || true
              fi
            else
              echo "Browser launch skipped (ALHALTE_NO_BROWSER is set or --no-browser specified)."
            fi
          '';
        };
      in
      {
        apps = {
          bistrot = {
            type = "app";
            program = "${bistrotRunner}/bin/bistrot-runner";
          };
          default = {
            type = "app";
            program = "${bistrotRunner}/bin/bistrot-runner";
          };
        };

        packages = {
          bistrot = bistrotRunner;
          default = bistrotRunner;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.sqlite
            # wasm tooling
            pkgs.wasm-pack
            pkgs.wasm-bindgen-cli
            pkgs.binaryen
            pkgs.wasmtime
            pkgs.wabt
            llvm.clang-unwrapped
            llvm.lld
            llvm.llvm
            # web
            pkgs.nodejs_22
            pkgs.pnpm
            pkgs.playwright-driver.browsers
            # tooling
            pkgs.gh
            pkgs.git
            pkgs.jq
            pkgs.cargo-nextest
            pkgs.cargo-insta
            pkgs.epubcheck
            pkgs.xdg-utils
            pkgs.curl
          ];

          WASI_SYSROOT = "${wasiSysroot}";
          WASI_LIBDIR = "${wasiSysroot}/lib/wasm32-wasip1";
          CC_wasm32_unknown_unknown = "${llvm.clang-unwrapped}/bin/clang";
          AR_wasm32_unknown_unknown = "${llvm.llvm}/bin/llvm-ar";
          CFLAGS_wasm32_unknown_unknown = wasiCflags;
          CC_wasm32_wasip1 = "${llvm.clang-unwrapped}/bin/clang";
          AR_wasm32_wasip1 = "${llvm.llvm}/bin/llvm-ar";
          CFLAGS_wasm32_wasip1 = wasiCflags;
          PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
          PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
          PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
          PLAYWRIGHT_VERSION = pkgs.playwright-driver.version;
        };
      });
}
