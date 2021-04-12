DEFAULT_GOAL := "all"

.PHONY: all
all: lint test

.PHONY: test
test:
	pkill python3 || true
	pytest -vv
	pkill python3 || true

.PHONY: lint
lint: clean
	black --line-length 100 .
	flake8 --max-line-length 100 .
	mypy --ignore-missing-imports .

.PHONY: clean
clean:
	git clean -fdx

.PHONY: docker
docker:
	#docker build . -f Dockerfile.fast -t docker.ruin.dev/innisfree
	docker build . -f Dockerfile -t docker.ruin.dev/innisfree
	docker push docker.ruin.dev/innisfree

.PHONY: run
run: docker
	docker run docker.ruin.dev/innisfree

.PHONY: deb
deb:
	dpkg-buildpackage -us -uc
	mv ../innisfree*_amd64.deb dist/
	find dist/ -type f -iname 'innisfree*.deb' | sort -n

.PHONY: push
push:
	rsync -a /home/user/gits/innisfree/ tau:/home/conor/innisfree/

