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
      in
      {
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
