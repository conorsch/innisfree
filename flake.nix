{
  description = "innisfree — expose local services on a public IPv4 address via a cloud server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
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

        # Build statically against musl so the resulting binary is portable
        # across distros and drops cleanly into a scratch container.
        muslTarget =
          if system == "aarch64-linux"
          then "aarch64-unknown-linux-musl"
          else "x86_64-unknown-linux-musl";

        # The cross-pkgs set provides a musl-targeted C toolchain. We use it
        # to source the linker that cargo invokes for the musl target.
        muslPkgs =
          if system == "aarch64-linux"
          then pkgs.pkgsCross.aarch64-multiplatform-musl
          else pkgs.pkgsCross.musl64;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ muslTarget ];
        };

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

        # Cargo expects the linker env var keyed by the upper-snake-cased target.
        targetUpper = lib.toUpper (lib.replaceStrings [ "-" ] [ "_" ] muslTarget);

        commonArgs = {
          inherit src;
          strictDeps = true;

          CARGO_BUILD_TARGET = muslTarget;
          # +crt-static produces a fully static ELF (no ld-musl interpreter).
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
          # Point cargo at the cross-musl C compiler for the linker step.
          "CARGO_TARGET_${targetUpper}_LINKER" =
            "${muslPkgs.stdenv.cc}/bin/${muslPkgs.stdenv.cc.targetPrefix}cc";

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
        # The binary is fully static, so no runtime closure beyond cacert
        # is required to make outbound TLS calls work.
        container = pkgs.dockerTools.buildLayeredImage {
          name = "innisfree";
          tag = "latest";
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
        # The `--features nix` static-linkage integration test lives in
        # `tests/static_linkage.rs` and is run from the devShell rather
        # than as a flake check, since it itself shells out to `nix build`
        # (which the flake-check sandbox cannot do).
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

        # The devShell uses the host (glibc) toolchain — faster incremental
        # builds during day-to-day work. Run `nix build` to produce the
        # static musl release binary.
        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            gum
            rustToolchain
            just
            wireguard-tools
            # `ldd` for the `--features nix` static-linkage test.
            glibc.bin
            cargo-deb
            cargo-edit
            cargo-nextest
            cargo-watch
            rust-analyzer
          ];

          RUST_LOG = "info";
        };
      });
}
