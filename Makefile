DEFAULT_GOAL := "all"

.PHONY: all
all: lint test

.PHONY: test
test:
	pkill python3 || true
	pytest -vv
	pkill python3 || true

.PHONY: lint
lint:
	flake8 --max-line-length 100 .
	black --line-length 100 .
	mypy --ignore-missing-imports .

.PHONY: clean
clean:
	git clean -fdx

.PHONY: docker
docker:
	docker build . -t docker.ruin.dev/innisfree
	docker push docker.ruin.dev/innisfree

.PHONY: deb
deb:
	dpkg-buildpackage -us -uc
	mv ../innisfree*_amd64.deb dist/
	find dist/ -type f -iname 'innisfree*.deb' | sort -n
