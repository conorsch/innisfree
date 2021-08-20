# Innisfree changelog

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
