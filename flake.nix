{
  description = "Alhalte - self-hosted VTT module suite";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, fenix, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        fx = fenix.packages.${system};
        # The devShell and WASM builds share the wasm target std. The host
        # package uses the same fenix toolchain so `cargo`/`rustc` match the
        # workspace's `rust-version`.
        toolchain = fx.combine [
          fx.stable.cargo
          fx.stable.rustc
          fx.stable.clippy
          fx.stable.rustfmt
          fx.stable.rust-src
          fx.stable.rust-analyzer
          fx.targets.wasm32-unknown-unknown.stable.rust-std
          fx.targets.wasm32-wasip1.stable.rust-std
          fx.targets.x86_64-pc-windows-gnu.stable.rust-std
        ];
        # crane builds the Cargo workspace in two steps (dependencies, then the
        # workspace crates) so dependency artifacts are cached across edits.
        # The host build uses the plain fenix stable cargo/rustc; the WASM build
        # uses the combined toolchain that carries the wasm32 std.
        craneLib = crane.mkLib pkgs;
        hostCraneLib = craneLib.overrideToolchain (_: fx.combine [ fx.stable.cargo fx.stable.rustc ]);
        wasmCraneLib = craneLib.overrideToolchain (_: toolchain);
        # The portable Windows bundle is built from x86_64-linux with the
        # mingw-w64 cross compiler. Other hosts still get the normal devShell.
        windowsTarget = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
        windowsCC = if windowsTarget then pkgs.pkgsCross.mingwW64.stdenv.cc else null;
        llvm = pkgs.llvmPackages;
        wasiSysroot = pkgs.symlinkJoin {
          name = "wasi-sysroot";
          paths = [ pkgs.pkgsCross.wasi32.wasilibc pkgs.pkgsCross.wasi32.wasilibc.dev ];
        };
        wasiCflags = "--target=wasm32-wasip1 --sysroot=${wasiSysroot} -isystem ${wasiSysroot}/include/wasm32-wasip1";

        # Sources for the crane builds. Only the Cargo workspace is included, so
        # edits under `web/`, `scripts/`, docs, etc. do not invalidate the Rust
        # dependency cache. Non-`.rs` files such as the `include_str!` CSS live
        # inside `crates/` and `extensions/`, so they are kept.
        rustSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./crates
            ./extensions
          ];
        };

        # Vendor the Cargo dependencies once and share them between the host and
        # WASM builds. The `aozora-rs` git checkout contains a crate whose
        # manifest points at `readme = "../README.md"`, which makes crane's
        # `cargo package -l` step fail, so the readme keys are stripped first.
        cargoVendorDir = craneLib.vendorCargoDeps {
          src = rustSrc;
          inherit cargoVendorDir;
          overrideVendorGitCheckout = ps: drv:
            if lib.any (p: lib.hasPrefix "git+https://github.com/kinoko0518/aozora-rs" p.source) ps then
              drv.overrideAttrs (_: {
                postPatch = ''
                  find . -name Cargo.toml -exec sed -i '/^readme[[:space:]]*=/d' {} +
                '';
              })
            else
              drv;
        };

        # The five long-lived/local binaries. `alhalte-cafe` is the one-shot
        # launcher; `alhalte-bistrot` only generates systemd units.
        hostCommonArgs = {
          pname = "alhalte";
          version = "0.1.0";
          src = rustSrc;
          inherit cargoVendorDir;
          strictDeps = true;
          cargoExtraArgs = "--locked --bins";
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl pkgs.sqlite ];
          doCheck = false;
        };
        hostCargoArtifacts = hostCraneLib.buildDepsOnly hostCommonArgs;
        alhaltePackage = hostCraneLib.buildPackage (hostCommonArgs // {
          cargoArtifacts = hostCargoArtifacts;
          meta = {
            description = "Alhalte binaries (cafe, bistrot, maitre-d, cellar, banquette)";
            mainProgram = "alhalte-cafe";
          };
        });

        # Saignant and the official extension wasm, built for
        # wasm32-unknown-unknown. The SPA build stages these files.
        wasmCommonArgs = {
          pname = "alhalte-wasm";
          version = "0.1.0";
          src = rustSrc;
          inherit cargoVendorDir;
          strictDeps = true;
          cargoExtraArgs = lib.concatStringsSep " " [
            "--locked"
            "-p alhalte-saignant"
            "-p alhalte-dsl"
            "-p markdown"
            "-p aozora"
            "-p coc-6"
            "-p coc-7"
            "-p emoklore"
          ];
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
          nativeBuildInputs = [
            llvm.clang-unwrapped
            llvm.llvm
            llvm.lld
          ];
          CC_wasm32_unknown_unknown = "${llvm.clang-unwrapped}/bin/clang";
          AR_wasm32_unknown_unknown = "${llvm.llvm}/bin/llvm-ar";
          CFLAGS_wasm32_unknown_unknown = "--target=wasm32-unknown-unknown";
          doCheck = false;
        };
        wasmCargoArtifacts = wasmCraneLib.buildDepsOnly wasmCommonArgs;
        wasmArtifacts = wasmCraneLib.buildPackage (wasmCommonArgs // {
          cargoArtifacts = wasmCargoArtifacts;
          doNotPostBuildInstallCargoBinaries = true;
          installPhaseCommand = ''
            mkdir -p $out
            find target/wasm32-unknown-unknown/release -maxdepth 1 -name '*.wasm' -type f -exec cp {} $out/ \;
          '';
        });

        # Production SPA for `maitre-d --web-root` and the glacier module.
        # `pnpm.fetchPnpmDeps` pins the workspace lockfile; the hash is updated
        # with `nix build .#argenterie-spa` when the lockfile changes.
        argenterieSpa = pkgs.stdenv.mkDerivation {
          pname = "alhalte-argenterie-spa";
          version = "0.1.0";
          src = ./web;

          pnpmDeps = pkgs.fetchPnpmDeps {
            pname = "alhalte-argenterie-spa";
            src = ./web;
            pnpm = pkgs.pnpm_11;
            fetcherVersion = 4;
            hash = "sha256-SG1KfR5vjDUwr1oXMww/0v/a+SDgvuBhMnwBzpT8eAs=";
          };

          nativeBuildInputs = [
            pkgs.pnpmConfigHook
            pkgs.pnpm_11
            pkgs.nodejs_22
            alhaltePackage
          ];

          buildPhase = ''
            runHook preBuild

            export HOME="$TMPDIR"

            mkdir -p alhalte-argenterie/public/wasm alhalte-argenterie/public/extensions
            cp ${wasmArtifacts}/alhalte_saignant.wasm alhalte-argenterie/public/wasm/

            cat > "$TMPDIR/extensions.toml" <<'EOF'
            [maitre-d]
            salt = "spa-build"
            local-login = true

            [cellar]

            [[extensions]]
            id = "alhalte-dsl"
            path = "${wasmArtifacts}/alhalte_dsl.wasm"
            enabled = true

            [[extensions]]
            id = "markdown"
            path = "${wasmArtifacts}/markdown.wasm"
            enabled = true

            [[extensions]]
            id = "aozora"
            path = "${wasmArtifacts}/aozora.wasm"
            enabled = true

            [[extensions]]
            id = "coc-6"
            path = "${wasmArtifacts}/coc_6.wasm"
            enabled = false

            [[extensions]]
            id = "coc-7"
            path = "${wasmArtifacts}/coc_7.wasm"
            enabled = false

            [[extensions]]
            id = "emoklore"
            path = "${wasmArtifacts}/emoklore.wasm"
            enabled = false
            EOF
            ALHALTE_ALLOW_PLAINTEXT_SECRETS=1 alhalte-cafe \
              --config "$TMPDIR/extensions.toml" \
              stage-extensions \
              --output-dir alhalte-argenterie/public/extensions

            pnpm --filter @alhalte/argenterie run build:app

            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -a alhalte-argenterie/dist-app/. $out/
            runHook postInstall
          '';
        };

        # `nix run .` starts the same cafe binary that the portable bundles use,
        # with the Nix-built SPA as --web-root. No systemd is involved.
        cafeWrapper = pkgs.writeShellApplication {
          name = "alhalte-cafe";
          runtimeInputs = [ alhaltePackage ];
          text = ''
            exec alhalte-cafe --web-root ${argenterieSpa} "$@"
          '';
        };

        saignantWasmRunner = pkgs.writeShellApplication {
          name = "saignant-wasm";
          runtimeInputs = [
            pkgs.bash
            toolchain
            llvm.clang-unwrapped
            llvm.lld
            llvm.llvm
            pkgs.coreutils
          ];
          text = ''
            export CC_wasm32_unknown_unknown="${llvm.clang-unwrapped}/bin/clang"
            export AR_wasm32_unknown_unknown="${llvm.llvm}/bin/llvm-ar"
            export CFLAGS_wasm32_unknown_unknown="--target=wasm32-unknown-unknown"
            exec bash "$PWD/scripts/build-saignant-wasm.sh"
          '';
        };

        # Portable Windows bundle: cross-compile the binaries, build the SPA,
        # stage extensions, and zip the folder.
        windowsBundleRunner = if windowsTarget then pkgs.writeShellApplication {
          name = "windows-bundle";
          runtimeInputs = [
            pkgs.bash
            pkgs.coreutils
            toolchain
            windowsCC
            llvm.clang-unwrapped
            llvm.lld
            llvm.llvm
            pkgs.nodejs_22
            pkgs.pnpm
            pkgs.zip
            pkgs.git
          ];
          text = ''
            export CC_wasm32_unknown_unknown="${llvm.clang-unwrapped}/bin/clang"
            export AR_wasm32_unknown_unknown="${llvm.llvm}/bin/llvm-ar"
            export CFLAGS_wasm32_unknown_unknown="--target=wasm32-unknown-unknown"
            mingw_cc="$(command -v x86_64-w64-mingw32-gcc)"
            mingw_ar="$(command -v x86_64-w64-mingw32-ar)"
            mingw_cxx="$(command -v x86_64-w64-mingw32-g++)"
            export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$mingw_cc"
            export CC_x86_64_pc_windows_gnu="$mingw_cc"
            export AR_x86_64_pc_windows_gnu="$mingw_ar"
            export CXX_x86_64_pc_windows_gnu="$mingw_cxx"
            exec bash "$PWD/scripts/build-windows-bundle.sh" "$@"
          '';
        } else null;

        # Portable folder for the current Linux/macOS host.
        portableBundleRunner = pkgs.writeShellApplication {
          name = "portable-bundle";
          runtimeInputs = [
            pkgs.bash
            pkgs.coreutils
            toolchain
            llvm.clang-unwrapped
            llvm.lld
            llvm.llvm
            pkgs.nodejs_22
            pkgs.pnpm
            pkgs.git
            pkgs.gnutar
            pkgs.gzip
          ];
          text = ''
            export CC_wasm32_unknown_unknown="${llvm.clang-unwrapped}/bin/clang"
            export AR_wasm32_unknown_unknown="${llvm.llvm}/bin/llvm-ar"
            export CFLAGS_wasm32_unknown_unknown="--target=wasm32-unknown-unknown"
            exec bash "$PWD/scripts/build-portable-bundle.sh" "$@"
          '';
        };
      in
      {
        apps = {
          default = {
            type = "app";
            program = "${cafeWrapper}/bin/alhalte-cafe";
          };
          cafe = {
            type = "app";
            program = "${cafeWrapper}/bin/alhalte-cafe";
          };
          bistrot = {
            type = "app";
            program = "${alhaltePackage}/bin/alhalte-bistrot";
          };
          saignant-wasm = {
            type = "app";
            program = "${saignantWasmRunner}/bin/saignant-wasm";
          };
          portable-bundle = {
            type = "app";
            program = "${portableBundleRunner}/bin/portable-bundle";
          };
        } // pkgs.lib.optionalAttrs windowsTarget {
          windows-bundle = {
            type = "app";
            program = "${windowsBundleRunner}/bin/windows-bundle";
          };
        };

        packages = {
          default = alhaltePackage;
          alhalte = alhaltePackage;
          argenterie-spa = argenterieSpa;
          cafe = cafeWrapper;
          bistrot = alhaltePackage;
          saignant-wasm = saignantWasmRunner;
          portable-bundle = portableBundleRunner;
        } // pkgs.lib.optionalAttrs windowsTarget {
          windows-bundle = windowsBundleRunner;
        };

        devShells.default = pkgs.mkShell ({
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
          ] ++ pkgs.lib.optional windowsTarget windowsCC ++ [ pkgs.zip ];

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
        } // pkgs.lib.optionalAttrs windowsTarget {
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${windowsCC}/bin/x86_64-w64-mingw32-gcc";
          CC_x86_64_pc_windows_gnu = "${windowsCC}/bin/x86_64-w64-mingw32-gcc";
          AR_x86_64_pc_windows_gnu = "${windowsCC}/bin/x86_64-w64-mingw32-ar";
          CXX_x86_64_pc_windows_gnu = "${windowsCC}/bin/x86_64-w64-mingw32-g++";
        });
      })
    // {
      nixosModules.default = import ./nix/alhalte-glacier/module.nix { alhalte = self; };

      nixosConfigurations.alhalte-example = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          self.nixosModules.default
          {
            system.stateVersion = "25.11";
            fileSystems."/" = {
              device = "/dev/sda1";
              fsType = "ext4";
            };
            boot.loader.grub.devices = [ "/dev/sda" ];
            services.alhalte = {
              enable = true;
              localLogin = true;
              salt = "example-instance-salt";
              serviceTokenFile = "/run/secrets/alhalte-service-token";
            };
          }
        ];
      };
    };
}
