## Porting to rust

* [x] Establish cli scaffolding
* [x] Detect DigitalOcean API var
* [x] Install DigitalOcean dependency
* [x] Create a droplet
* [x] Wait on droplet creation
* [x] Print droplet IPv4 addr

* [x] Port template logic for cloudinit
* [x] Create droplet from cloudinit data
* [x] Write out cloudinit to disk, for debugging
* [x] Write test for cloudinit header

* [x] Port SSH keygen
* [x] Port SSH cloudinit
* [x] Port SSH local kp
* [x] Port WG addr
* [x] Port WG cloudinit
* [x] Port WG up
* [x] Write tests for keypair matching
* [x] Silence command output in wg up

* [x] Add proxy code
* [x] Wire up "proxy" subcommand
* [x] Integrate proxy in up subcommand

* [x] Port deb pkg logic for rust
* [x] Pare down Cargo.toml proxy code

* [x] SSH should use tmpfiles, not clobber primary dir
* [ ] Package upgrade should be async
* [ ] Add cleanup methods
* [ ] Catch ctrl+c to cleanup
* [x] Tune nginx config, workers auto
* [ ] Wireguard config should be a /30

* [ ] Add iptables rules to wg to block all but authorized
