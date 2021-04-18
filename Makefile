DEFAULT_GOAL := "build"

.PHONY: all
all: lint test build

.PHONY: build
build:
	cargo build
	./target/debug/innisfree up

.PHONY: test
test:
	cargo test

.PHONY: lint
lint: clean
	cargo fmt

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
deb: clean
	dpkg-buildpackage -us -uc
	mv ../innisfree*_amd64.deb dist/
	find dist/ -type f -iname 'innisfree*.deb' | sort -n
