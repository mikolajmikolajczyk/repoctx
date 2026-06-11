{
  description = "repoctx — AI-oriented repository intelligence CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            # Rust toolchain (stable from nixpkgs; swap to rust-overlay if pinning matters)
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer

            # Native deps for rusqlite / libsqlite3-sys
            pkg-config
            sqlite

            # Project tooling
            pre-commit
            shellcheck
            gitleaks
            markdownlint-cli

            # Benchmarks (scripts/bench.sh)
            hyperfine
            jq
            python3
          ];

          shellHook = ''
            export RUST_BACKTRACE=1
          '';
        };
      });
}
