containers:
    podman build -t innisfree-debian -f containers/Containerfile-debian .
    podman build -t innisfree-fedora -f containers/Containerfile-fedora .
    # skipping alpine build, incomplete...
    # podman build -t innisfree-alpine -f containers/Containerfile-alpine .

integration:
    cargo test -- --ignored
