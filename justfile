# build container image
container:
    nix build .#container

# run unit tests
test:
    cargo test

# run linters
check:
  cargo check --all-features --all-targets
  cargo clippy --all-features --all-targets
  cargo fmt --check

# run all tests
integration:
    cargo test -- --ignored
