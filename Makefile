DEFAULT_GOAL := "build"

.PHONY: all
all: lint test build


.PHONY: run
run: build
	./i up

.PHONY: build
build: install-deps
	cargo build
	sudo setcap CAP_NET_BIND_SERVICE=+ep ./target/debug/innisfree

.PHONY: test
test:
	cargo test

.PHONY: lint
lint:
	cargo fmt
	cargo clippy

.PHONY: clean
clean:
	cargo clean
	git clean -fdX

.PHONY: docker
docker:
	#docker build . -f Dockerfile.fast -t docker.ruin.dev/innisfree
	docker build . -f Dockerfile -t docker.ruin.dev/innisfree
	docker push docker.ruin.dev/innisfree-rust

.PHONY: deb
deb:
	dpkg-buildpackage -us -uc
	mv ../innisfree*_amd64.deb dist/
	find dist/ -type f -iname 'innisfree*.deb' | sort -n

.PHONY: install-deps
install-deps:
	sudo apt install -y libssl-dev libcap2-bin

.PHONY: ci
ci: install-deps lint test
	cargo check
	cargo check --release
	cargo build
	cargo build --release

.PHONY: push
push:
	rsync -a --info=progress2 --exclude "target/*" --delete-after /home/user/gits/innisfree-rust/ tau:/home/conor/innisfree-rust/
