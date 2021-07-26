innisfree
==========

A tool to aid in self-hosting. Expose local services on your computer,
via a public IPv4 address.

<p align="center">
  <img src="docs/assets/demo-up.gif"/>
</p>


Why?
----

Most of the data I maintain is local, within my house, or otherwise on a machine
that I control. Uploading that information to a cloud server doesn't always make sense.
For one, it's not private: the cloud provider has direct access to all the files
I upload. Second, if I have large amounts of data, such as a music collection,
it's both expensive and inefficient to upload all that data to the cloud simply
so that I can access it remotely.

Mostly, I just want an IP address tied to a service I'm running.
I don't want to publish my home IP in DNS records. I don't want to
open ports on my home router to allow traffic in.

How it works
------------
When you run `innisfree up`, the program performs the following steps:

1. Checks for `DIGITALOCEAN_API_TOKEN` env var, so it can access the [DigitalOcean] cloud provider.
2. Generates keypairs locally, for trusted connections over SSH and Wireguard.
3. Creates a new cloud server, configured with those keypairs.
4. Builds a [Wireguard] connection between your local computer and the server.
5. Configures nginx on the server, to pass traffic from the public IP of the server
   to select services you're running locally (by default, `8080/TCP,443/TCP`).
6. If a `--dest-ip` was specified, configures a local proxy to pass traffic
   from the local Wireguard interface to another service locally.
   Useful when the local service is running on an address other than localhost.

Installation
------------
There are deb packages available in the Releases page on this repo.
You can install directly from source:

```
# Build a deb package and install it locally
make install
```

Requirements
------------

1. Linux-only. Even if the binary compiles under macOS, userspace
   implementations of Wireguard are still up-and-coming.
2. [Wireguard]. For most modern Linux distros, this is available
   out of the box. Notably, Debian Stable Buster 10 lacks it,
   but it's available in the buster-backports repo. Run
   `innisfree doctor` to check support your machine.
3. A [DigitalOcean] cloud account, to create a server.

Usage
-----

```
USAGE:
    innisfree [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    doctor    Run checks to evaluate platform support
    help      Prints this message or the help of the given subcommand(s)
    ip        Display IPv4 address for cloud node
    proxy     Start process to forward traffic, assumes tunnel already up
    ssh       Open interactive SSH shell on cloud node
    up        Create new innisfree tunnel
```

Running as a service
--------------------

The deb package ships with a systemd config file.
To use it, choose a unique name for the service (e.g. `foo`),
and make sure your API token is exported:

```
sudo mkdir -p /etc/systemd/system/innisfree@foo.service.d/
printf '[Service]\nEnvironment="DIGITALOCEAN_API_TOKEN=%s"\n' "$DIGITALOCEAN_API_TOKEN" > /etc/systemd/system/innisfree@foo.service.d/override.conf
sudo systemctl daemon-reload
sudo systemctl restart innisfree@foo
sudo journalctl -af -u innisfree@foo
```

You can also override more settings:

```
cat /etc/systemd/system/innisfree\@minikube.service.d/override.conf
[Service]
Environment="DIGITALOCEAN_API_TOKEN=<REDACTED>"

ExecStart=
ExecStart=/bin/bash -c "innisfree up --floating-ip 1.2.3.4 --dest-ip $(su conorsch -c 'minikube ip') -p 443/TCP --name k8s"
```

The above will proxy a connection to a local minikube installation.

What's with the name?
---------------------

It's from the [Yeats poem](https://poets.org/poem/lake-isle-innisfree):

> I will arise and go now, and go to Innisfree,<br>
> And a small cabin build there, of clay and wattles made:<br>
> Nine bean-rows will I have there, a hive for the honey-bee;<br>
> And live alone in the bee-loud glade.<br>

The idea is that in context of the internet, my own home is already the "bee-loud glade":
I don't need to upload all my data into the hustle and bustle of cloud computing.
Just give me an IP, so I can share data with others, and that's enough.


License
----
AGPLv3

[Wireguard]:https://www.wireguard.com
[DigitalOcean]:https://www.digitalocean.com
[minikube]:https://github.com/kubernetes/minikube
