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
* [x] Configure unattended-upgrades
* [x] Add cleanup methods - dir
* [x] Add cleanup methods - droplet
* [x] Add cleanup methods - wg
* [x] Catch ctrl+c to cleanup
* [x] Tune nginx config, workers auto
* [x] Wireguard config should be a /30
* [x] SSH privkey should be 600
* [x] SSH pubkey file should contain pubkey, not privkey
* [x] SSH commands don't seem to report failure
* [x] Wire up floating ip
* [x] Wg command should fail

* [x] Support local ip service forwarding (i.e. no-proxy)
* [x] Add iptables rules to wg to block all but authorized
* [x] Make ip command fail if server doesnt exit
* [x] Make ssh command fail if server doesnt exit
* [x] Add lots of results for better error handling
* [x] Add doctor subcommand for checking

* [x] Service stop should clean up resources
* [ ] Support SIGTERM and SIGKILL signals
* [x] Make 'release' builds reproducible
* [ ] Make deb package builds reproducible
* [x] Use a build.rs file for setting remap on rustcflags https://doc.rust-lang.org/cargo/reference/build-scripts.html
      Turns out maybe this isn't possible: RUSTFLAGS must be set above the cargo context in which build.rs runs.
      So, settling on a .env file for now to set RUSTFLAGS for reproducible builds.

* [x] Use std::net::IpAddr
* [x] Use std::net::SocketAddr
