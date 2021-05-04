DEFAULT_GOAL := "all"

.PHONY: all
all: lint test build

.PHONY: run
run: build
	cargo run -- up

.PHONY: build
build: install-deps
	cargo build

.PHONY: test
test:
	cargo test

.PHONY: install
install:
	rm -vf target/debian/innisfree*.deb
	$(MAKE) deb
	sudo dpkg -i target/debian/innisfree*.deb

.PHONY: lint
lint:
	cargo fmt
	cargo clippy

.PHONY: clean
clean:
	cargo clean
	git clean -fdX

.PHONY: deb
deb:
	cargo deb

.PHONY: install-deps
install-deps:
	sudo apt install -y libssl-dev libcap2-bin reprotest lld
	cargo deb --version || cargo install cargo-deb

.PHONY: ci
ci: install-deps lint test
	cargo check
	cargo check --release
	cargo build
	cargo build --release
	$(MAKE) deb


.PHONY: reprotest
reprotest: install-deps
	reprotest \
		--variations "-kernel, -user_group, -domain_host, -home" \
		--min-cpus=99999 --auto-build -c ". $$HOME/.cargo/env && . ./.env && unset LD_PRELOAD && rustup default stable && cargo build --release" . "target/release/innisfree"

.PHONY: reprotest-deb
# export SOURCE_DATE_EPOCH="$(dpkg-parsechangelog -STimestamp)"
reprotest-deb:
	echo "doesn't work yet, since cargo-deb has no support for 'SOURCE_DATE_EPOCH'"
	echo "two variations are prominent: timestamp metadata, and ordering of the 'Depends' field values."
	reprotest \
		--variations "-kernel, -user_group, -domain_host, -home" \
		--min-cpus=99999 --auto-build -c ". $HOME/.cargo/env && unset LD_PRELOAD && rustup default stable && make deb" . "target/debian/*.deb"
