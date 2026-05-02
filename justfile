# build container images
containers:
    podman build -t innisfree-debian -f containers/Containerfile-debian .
    podman build -t innisfree-fedora -f containers/Containerfile-fedora .
    # skipping alpine build, incomplete...
    # podman build -t innisfree-alpine -f containers/Containerfile-alpine .

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
