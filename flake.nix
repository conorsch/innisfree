{
  description = "innisfree — expose local services on a public IPv4 address via a cloud server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay }:
    # innisfree is Linux-only (Wireguard kernel module, wg-quick).
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        inherit (pkgs) lib;

        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # The crate uses include_str!("../../files/...") to embed templates,
        # so the default cargo-source filter (which strips non-Rust files)
        # would break the build. Whitelist anything under `files/`.
        src = lib.cleanSourceWith {
          src = ./.;
          name = "source";
          filter = path: type:
            (lib.hasInfix "/files/" path)
            || (craneLib.filterCargoSources path type);
        };

        commonArgs = {
          inherit src;
          strictDeps = true;

          nativeBuildInputs = [ pkgs.pkg-config ];
          # The reqwest dep is built with native-tls (openssl), and pnet
          # links against system libpcap. Include both so the build succeeds.
          buildInputs = [ pkgs.openssl ];

          # Tests shell out to `wg`, so they only run inside the devShell;
          # the package build skips them to stay hermetic and fast.
          doCheck = false;
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        innisfree = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          meta = {
            description = "Expose local services on a public IPv4 address via a cloud server";
            homepage = "https://github.com/conorsch/innisfree";
            license = lib.licenses.agpl3Only;
            mainProgram = "innisfree";
            platforms = lib.platforms.linux;
          };
        });

        # Optional container image. Build with:
        #   nix build .#container
        # Load with:
        #   docker load < $(nix build --no-link --print-out-paths .#container)
        container = pkgs.dockerTools.buildLayeredImage {
          name = "innisfree";
          tag = "latest";
          # cacert provides /etc/ssl/certs so reqwest can verify TLS.
          contents = [ innisfree pkgs.cacert ];
          config = {
            Entrypoint = [ (lib.getExe innisfree) ];
            Env = [ "RUST_LOG=info" ];
          };
        };
      in {
        packages = {
          default = innisfree;
          inherit innisfree container;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = innisfree;
          name = "innisfree";
        };

        # `nix flake check` builds the package and runs clippy + rustfmt.
        checks = {
          inherit innisfree;

          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-features --all-targets -- --deny warnings";
          });

          fmt = craneLib.cargoFmt {
            inherit src;
          };
        };

        devShells.default = craneLib.devShell {
          inputsFrom = [ innisfree ];

          packages = with pkgs; [
            just
            wireguard-tools
            cargo-watch
            cargo-edit
            cargo-deb
            rust-analyzer
          ];

          RUST_LOG = "info";
        };
      });
}
