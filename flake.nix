{
  description = "Espresso Decentralized Sequencer";

  nixConfig = {
    extra-substituters = [
      "https://espresso-systems-private.cachix.org"
      "https://nixpkgs-cross-overlay.cachix.org"
    ];
    extra-trusted-public-keys = [
      "espresso-systems-private.cachix.org-1:LHYk03zKQCeZ4dvg3NctyCq88e44oBZVug5LpYKjPRI="
      "nixpkgs-cross-overlay.cachix.org-1:TjKExGN4ys960TlsGqNOI/NBdoz2Jdr2ow1VybWV5JM="
    ];
  };

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  inputs.fenix.url = "github:nix-community/fenix";
  inputs.fenix.inputs.nixpkgs.follows = "nixpkgs";

  inputs.nixpkgs-cross-overlay = {
    url = "github:alekseysidorov/nixpkgs-cross-overlay";
    inputs.flake-utils.follows = "flake-utils";
  };

  inputs.flake-utils.url = "github:numtide/flake-utils";

  inputs.flake-compat.url = "github:edolstra/flake-compat";
  inputs.flake-compat.flake = false;

  inputs.pre-commit-hooks.url = "github:cachix/pre-commit-hooks.nix";

  inputs.foundry.url = "github:shazow/foundry.nix/monthly"; # Use monthly branch for permanent releases

  outputs = { self, nixpkgs, rust-overlay, nixpkgs-cross-overlay, flake-utils, pre-commit-hooks, fenix, foundry, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        RUST_LOG = "info,libp2p=off,isahc=error,surf=error";
        RUSTFLAGS = " --cfg async_executor_impl=\"async-std\" --cfg async_channel_impl=\"async-std\"";
        RUST_BACKTRACE = 1;
        RUST_LOG_FORMAT = "full";
        overlays = [
          (import rust-overlay)
          foundry.overlay
        ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      with pkgs;
      {
        checks = {
          pre-commit-check = pre-commit-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              doc = {
                enable = true;
                description = "Generate figures";
                entry = "make doc";
                types_or = [ "plantuml" ];
                pass_filenames = false;
              };
              cargo-fmt = {
                enable = true;
                description = "Enforce rustfmt";
                entry = "cargo fmt --all";
                pass_filenames = false;
              };
              cargo-sort = {
                enable = true;
                description = "Ensure Cargo.toml are sorted";
                entry = "cargo sort -g -w";
                pass_filenames = false;
              };
              cargo-clippy = {
                enable = true;
                description = "Run clippy";
                entry = "cargo clippy --workspace --all-features --all-targets -- -D warnings";
                pass_filenames = false;
              };
            };
          };
        };
        devShells.default =
          let
            stableToolchain = pkgs.rust-bin.stable.latest.minimal.override {
              extensions = [ "rustfmt" "clippy" "llvm-tools-preview" "rust-src" ];
            };
            # nixWithFlakes allows pre v2.4 nix installations to use
            # flake commands (like `nix flake update`)
            nixWithFlakes = pkgs.writeShellScriptBin "nix" ''
              exec ${pkgs.nixFlakes}/bin/nix --experimental-features "nix-command flakes" "$@"
            '';
          in
          mkShell
            {
              buildInputs = [
                # Rust dependencies
                pkgconfig
                openssl
                curl
                protobuf # to compile libp2p-autonat
                stableToolchain

                # Rust tools
                cargo-audit
                cargo-edit
                cargo-watch
                cargo-sort
                just
                fenix.packages.${system}.rust-analyzer

                # Ethereum
                foundry-bin
                go-ethereum

                # Tools
                nixWithFlakes
                entr
                jq
                nodejs
                nodePackages.pnpm

                # Figures
                graphviz
                plantuml
                coreutils
              ] ++ lib.optionals stdenv.isDarwin [ darwin.apple_sdk.frameworks.SystemConfiguration ];
              shellHook = ''
                # Prevent cargo aliases from using programs in `~/.cargo` to avoid conflicts
                # with rustup installations.
                export CARGO_HOME=$HOME/.cargo-nix
              '' + self.checks.${system}.pre-commit-check.shellHook;
              RUST_SRC_PATH = "${stableToolchain}/lib/rustlib/src/rust/library";
              inherit RUST_LOG RUST_LOG_FORMAT RUSTFLAGS RUST_BACKTRACE;
            };
        devShells.crossShell =
          let
            localSystem = system;
            crossSystem = { config = "x86_64-unknown-linux-musl"; useLLVM = true; isStatic = true; };
            pkgs = import "${nixpkgs-cross-overlay}/utils/nixpkgs.nix" {
              inherit overlays localSystem crossSystem;
            };
          in
          import ./cross-shell.nix { inherit pkgs RUST_LOG RUST_LOG_FORMAT RUSTFLAGS RUST_BACKTRACE; };
      }
    );
}
