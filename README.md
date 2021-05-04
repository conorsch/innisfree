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

Usage
-----

```
USAGE:
    innisfree [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    help     Prints this message or the help of the given subcommand(s)
    ip       Display IPv4 address for cloud node
    proxy    Start process to forward traffic, assumes tunnel already up
    ssh      Open interactive SSH shell on cloud node
    up       Create new innisfree tunnel
```


License
----
AGPLv3

[Wireguard]:https://www.wireguard.com
[DigitalOcean]:https://www.digitalocean.com
