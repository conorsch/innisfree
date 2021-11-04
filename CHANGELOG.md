# Innisfree changelog

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
