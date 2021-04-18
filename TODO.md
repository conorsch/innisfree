## Porting to rust

* [x] Establish cli scaffolding
* [x] Detect DigitalOcean API var
* [x] Install DigitalOcean dependency
* [x] Create a droplet
* [x] Wait on droplet creation
* [x] Print droplet IPv4 addr

* [ ] Find cloudinit support in crate
  * PROBLEM: user_data args only supports bool
  * Might need to fork and add support
* [ ] Port template logic for cloudinit
* [ ] Create droplet from cloudinit data

* [ ] Port SSH logic
* [ ] Port WG logic

