DEFAULT_GOAL := "none"

.PHONY: none
none:


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
	cargo deb

.PHONY: install-deps
install-deps:
	sudo apt install -y libssl-dev libcap2-bin reprotest
	# cargo install cargo-deb

.PHONY: ci
ci: install-deps lint test
	cargo check
	cargo check --release
	cargo build
	cargo build --release
	$(MAKE) deb

.PHONY: reprotest
reprotest: install-deps
	reprotest -c "make build" . "target/debug/innisfree"

.PHONY: push
push:
	rsync -a --info=progress2 --exclude "target/*" --delete-after /home/user/gits/innisfree-rust/ tau:/home/conor/innisfree-rust/

.PHONY: deploy
deploy: deb
	rsync -a --info=progress2 -e ssh /home/user/gits/innisfree-rust/target/debian/innisfree_0.1.1_amd64.deb baldur:pkgs/
	ssh baldur "sudo dpkg -i pkgs/innisfree*.deb"
