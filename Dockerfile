# Grab doctl from upstream container.
# Should be using a pure python API, too lazy right now
ARG DOCTL_VERSION=1.46.0
FROM digitalocean/doctl:${DOCTL_VERSION} AS doctl


# Build the innisfree debian package.
# Possible to install from source, this is a bit cleaner
# in terms of explicit dependency management.
FROM debian:buster AS builder
RUN apt-get update && apt-get upgrade -y
RUN apt-get install -y dh-virtualenv python3-dev make git build-essential debhelper devscripts equivs

# Build that deb pkg!
COPY . /code
WORKDIR /code
RUN ls -l
RUN make deb
RUN dpkg -i /code/dist/*.deb
RUN innisfree --help

# Final container: copy in doctl & innisfree pkg, install
FROM python:buster
ARG UID=1000
ARG GID=1000
ARG USERNAME=user

RUN groupadd -g ${GID} ${USERNAME} && useradd -m -d /home/${USERNAME} -g ${GID} -u ${UID} ${USERNAME}
COPY --from=doctl /app/doctl /usr/bin/doctl
RUN apt-get update
RUN apt-get install -y python3-dev python3-pip

COPY --from=builder /code/dist/*.deb /tmp/
RUN apt-get install -y -f /tmp/innisfree*.deb

RUN apt-get install -y openssh-client

USER ${USERNAME}
CMD ["/usr/bin/innisfree"]
