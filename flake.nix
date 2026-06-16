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

        repoctx = pkgs.rustPlatform.buildRustPackage {
          pname = "repoctx";
          version = "0.11.6";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];

          # Integration tests spawn `repoctx --repo <tempdir> ...`; skip them
          # in the sandbox, they're already exercised under `nix develop`.
          doCheck = false;

          meta = with pkgs.lib; {
            description = "AI-oriented repository intelligence CLI";
            homepage = "https://github.com/mikolajmikolajczyk/repoctx";
            license = licenses.lgpl3Plus;
            mainProgram = "repoctx";
          };
        };
      in
      {
        packages.default = repoctx;
        packages.repoctx = repoctx;

        apps.default = {
          type = "app";
          program = "${repoctx}/bin/repoctx";
        };

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

            # Benchmarks (scripts/bench.sh + scripts/agent-bench)
            hyperfine
            jq
            python3
            bats
            ripgrep
          ];

          shellHook = ''
            export RUST_BACKTRACE=1
          '';
        };
      });
}
