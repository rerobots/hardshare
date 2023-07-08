---
title: Installation Instructions
---

# Installation Instructions

## Summary

The goals of this page are to provide more detail than [Quickstart](/quickstart) and to
treat special cases. If [Quickstart](/quickstart) worked for you, then you can safely
skip this page.

The details vary depending upon the hardware that you want to share. These
instructions are work in progress, and contributions and feedback are welcome.
Please open a ticket at <https://github.com/rerobots/hardshare/issues>

Installation instructions are provided for macOS and modern GNU/Linux distributions, such
as [Ubuntu](https://ubuntu.com/download/desktop). We are working to support other kinds of hosts, including Windows and FreeBSD.
If your host is not supported well, [please tell us](https://rerobots.net/contact).


## Main Aspects

The main aspects to an operational `hardshare` installation:

1. API token for a [rerobots](https://rerobots.net/) user account,
2. `hardshare` client,
3. a container provider (also known as *cprovider*),
4. rules around instance initialization, termination, and filtering.

To begin, initialize a new configuration

```bash
hardshare init
```


### API Tokens

Instructions about managing API tokens are [in the rerobots Web Guide](
https://docs.rerobots.net/web/making-and-revoking-api-tokens). The
token that you create at <https://rerobots.net/tokens> and download is saved to
your local hardshare configuration. As such, the default expiration time might
be too small for your application. Download the token, and add it

```bash
hardshare config --add-token path/to/your/jwt.txt
```


### Containers

Hardshare shares hardware among remote users through containers. The term
*container* in the context of hardshare includes Linux containers. Supporting
software that facilitates containers in hardshare are known *cproviders*. For new
users, Docker is a good first cprovider to try and is the default in a
newly installed `hardshare` client configuration.

Finally, releases of the client are available at <https://github.com/rerobots/hardshare/releases>
When installed, you should be able to get the version from the
command-line interface (CLI)

```bash
hardshare version
```


## Prepare a cprovider

### Docker

The default cprovider is `docker`, which works for installations of Docker
from the package management systems of popular distributions.
For [rootless Docker](https://docs.docker.com/engine/security/rootless/),
```bash
hardshare config --cprovider docker-rootless
```

In most cases, Docker images are available via Docker Hub. The correct image to
use depends on your host architecture. On Linux, you can do

```bash
uname -m
```

to find this. For example, on [Raspberry Pi](
https://www.raspberrypi.com/products/raspberry-pi-4-model-b/specifications/)
this would be `armv7l`, so Docker image tags that begin with `armv7l-` can be
used. To get the latest release of the base generic image

```bash
docker pull rerobots/hs-generic:armv7l-latest
```

which pulls the image from [Docker Hub](https://hub.docker.com/r/rerobots/hs-generic).
To declare this image in the local hardshare configuration

```bash
hardshare config --assign-image rerobots/hs-generic:armv7l-latest
```

Many consumer "desktop" and "laptop" computers have the `x86_64` architecture,
so the corresponding image is instead `rerobots/hs-generic:x86_64-latest`.
Recent versions of RaspberryPi have `aarch64` (also known as `arm64`).

Images in this registry are defined by Dockerfiles [under the directory
devices/ of the sourcetree](https://github.com/rerobots/hardshare/tree/main/devices).
To build the image from source files, use the command
given in the comments of the Dockerfile. For example,

```bash
docker build -t rerobots/hs-generic:latest -f Dockerfile .
```


### Podman

For many operations, [podman](https://podman.io/) is a drop-in replacement for `docker`. To switch
to it with an existing hardshare configuration (created as described above),

```bash
hardshare config --cprovider podman
```

Then, [the section about Docker](#docker) can be
followed by replacing `docker` with `podman`.


### LXD

For many operations, [LXD](https://linuxcontainers.org/lxd/) is a drop-in
replacement for `docker`. To switch to it with an existing hardshare
configuration (created as described above),

```bash
hardshare config --cprovider lxd
```

Then, [the section about Docker](#docker) can be
followed by replacing `docker` with `lxc`.


## Include Devices

For example,

```bash
hardshare config --add-raw-device /dev/ttyUSB0
```


## Access Rules

Each robot shared through [rerobots](https://rerobots.net/) is subject to access
rules about who can do what with it.
These rules are said to define *capabilities*. The decision
sequence for a user `username` trying to perform some `action` is the
following:

1. if there is a rule about `action` explicitly for `username`, then apply it;
2. else, if there is a rule about  `action` that is for a class of users of which `username` is a member, then apply it;
3. else, if there is a rule about `action` that targets all users (indicated by `*`), then apply it;
4. else (no match), default to not permit.

The most simple kind of rule is whether or not to allow someone to remotely
access a device. When a new device is registered, a single rule is created that
permits only you (i.e., your user account) to create instances. To get the list
of access rules

```bash
hardshare rules -l
```

which should only have 1 item under `rules`: a capability `CAP_INSTANTIATE`
and your username. To allow other users

```bash
hardshare rules --permit-all
```


## What Next?

At this stage, you have a ready-to-use hardshare host. Next:

* [Attach External Camera](/attach_camera) to provide video streams of your hardware.
* Go to <https://rerobots.net/hardshare> and create your first sandbox.
* If you have a [PlatformIO](https://docs.platformio.org/en/latest/what-is-platformio.html)-compatible board (for example, [ESP32](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/)), do the [Tutorial: ESP32](/tutorials/esp32).
* If you have a [Misty](https://www.mistyrobotics.com/) robot, do the [Tutorial: Misty](/tutorials/misty).
