# Innisfree changelog

## 0.3.0
* Add support for different local and remote port pairs, e.g. `80:8000`.
* Dev: refactor InnisfreeServer as trait, implemented on Droplet.
* Dev: use `tracing-subscriber` for logging
* Dev: replace unwrap/expect use with anyhow error handling.
* Dev: cargo update

## 0.2.16

* Use Debian Stable (11 Bullseye), rather than Ubuntu LTS, for cloud image
* Bugfix: add all API pubkeys, not just the first
* Dev only: don't error out on tests if no API key is present
* Dev only: prune unused fields from structs (thanks, clippy!)

## 0.2.15

* Add all pre-existing SSH pubkeys from DO account to server
  (enables log-in from other tooling)

## 0.2.14

* Post ephemeral SSH key to DO account (avoids new instance emails)

## 0.2.13

* [mistaken release, same as 0.2.12]

## 0.2.12

* Statically links all library dependencies
* Update all dependencies to latest
* Dev only: update release tooling

## 0.2.11

* Ensure Wireguard subnets are /30
* Bugfix: clean config dirs on destroy
* Bugfix: ssh command handles --name flag

## 0.2.10

* Support multiple tunnels on same host
* Bugfix: default server name is `innisfree` again, (was briefly `innisfree-innisfree`)
* Dev only: more explicit typing for IP addresses throughout

## 0.2.9

* Enable unattended-upgrades
* Support graceful termination in systemd service

## 0.2.8

* Updates all dependencies to latest
* Uses async function calls where possible
* Debian package reloads systemd, and loosens version dependencies

## 0.2.7

* Publish to crates.io

## 0.2.6

* Add systemd service support
* Make cli args configurable via env var
