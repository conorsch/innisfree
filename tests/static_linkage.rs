//! Verifies the `innisfree` binary produced by the project's flake is
//! fully statically linked.
//!
//! The flake targets `*-unknown-linux-musl` with `+crt-static`, so the
//! resulting binary should have no dynamic linker dependencies. The
//! assertion is meaningless against the regular cargo debug build (which
//! is glibc-dynamic), so this test invokes `nix build` to obtain the
//! actual artifact rather than relying on `CARGO_BIN_EXE_*`.
//!
//! Gate behind the `nix` feature so day-to-day `cargo test` runs aren't
//! held to it. Run with:
//!
//!     cargo test --features nix --test static_linkage
//!
//! Requires `nix` and `ldd` on PATH. Both are present in the flake's
//! devShell.
#![cfg(feature = "nix")]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn binary_is_statically_linked() {
    let binary = nix_build_innisfree();

    let output = Command::new("ldd")
        .arg(&binary)
        .output()
        .expect("failed to run `ldd`; ensure glibc is on PATH");

    // glibc's ldd prints to either stream depending on version, so check both.
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        report.contains("statically linked"),
        "expected `ldd {}` to report 'statically linked'; got:\n{report}",
        binary.display(),
    );
}

/// Build the release binary via the project's flake and return its path
/// inside the nix store. Uses `--no-link` so we don't litter the working
/// tree with a `result` symlink.
fn nix_build_innisfree() -> PathBuf {
    let output = Command::new("nix")
        .args([
            "build",
            ".#innisfree",
            "--no-link",
            "--print-out-paths",
        ])
        .output()
        .expect("failed to invoke `nix build`; ensure nix is on PATH");

    assert!(
        output.status.success(),
        "`nix build .#innisfree` failed:\n{}",
        String::from_utf8_lossy(&output.stderr),
    );

    let store_path = String::from_utf8(output.stdout)
        .expect("`nix build` output is not valid UTF-8");
    PathBuf::from(store_path.trim()).join("bin").join("innisfree")
}
