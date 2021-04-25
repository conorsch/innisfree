DEFAULT_GOAL := "build"

.PHONY: all
all: lint test build

.PHONY: build
build:
	cargo run -- up

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

.PHONY: run
run: docker
	docker run docker.ruin.dev/innisfree-rust

.PHONY: deb
deb:
	dpkg-buildpackage -us -uc
	mv ../innisfree*_amd64.deb dist/
	find dist/ -type f -iname 'innisfree*.deb' | sort -n

.PHONY: install-deps
install-deps:
	sudo apt install -y libssl-dev

.PHONY: push
push:
	rsync -a --info=progress2 --exclude "target/*" --delete-after /home/user/gits/innisfree-rust/ tau:/home/conor/innisfree-rust/
