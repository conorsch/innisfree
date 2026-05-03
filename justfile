# build container image
container:
    nix build .#container

# run unit tests
test:
    cargo nextest run

# run linters
check:
  cargo check --all-features --all-targets
  cargo clippy --all-features --all-targets
  cargo fmt --check

alias lint := check

# run the live integration test (builds, applies setcap via sudo, then
# `cargo test --test integration_test --ignored`). Provisions a real
# DigitalOcean droplet — DIGITALOCEAN_API_TOKEN must be set.
integration:
    ./tools/test-runner
