---
title: Add-ons
---

# Add-ons

All workspace deployments provided by [rerobots](https://rerobots.net/) and all devices provided by
hardshare users support the core API. Additional features, referred to as
*add-ons*, can also be provided by hardshare owners if sufficient hardware
requirements are satisfied.


## cam

Many workspaces include cameras. The add-on `cam` enables working with cameras
via `api.rerobots.net` and embedding of video feeds in webpages, such as
sandboxes hosted on `rerobots.net`. If the entire camera is part of a single
workspace deployment, then typically the camera device file will be created
inside the filesystem of an instance, and the add-on `cam` will operate on
this file.

Many hardshare users want to share a camera across multiple workspaces, or to
only expose a subset of the image to their remote users. For these cases,
`hardshare` provides special features described in the section [Attach External Camera](/attach_camera).


## cmdsh

The add-on `cmdsh` provides remote shells via WebSockets. This is motivated to
provide `ssh`-like features from contexts where `ssh` is not available, for
example in Web apps. This add-on is required for sandbox kinds that include a
browser-embedded terminal: `xterm` and `editor-xterm`.

To add it from the CLI

```bash
hardshare addon-cmdsh --add
```

and to remove it, `hardshare addon-cmdsh --rm`.

To use `cmdsh`, instances must be created from a compatible image. The default
Docker image `rerobots/hs-generic` (defined by generic/Dockerfile in the
[under the directory devices/ of the sourcetree](https://github.com/rerobots/hardshare/tree/master/devices)) supports `cmdsh`.


## VNC

VNC is a system for remote access to a desktop display, keyboard, and mouse,
following the protocol defined in [RFC 6143](https://tools.ietf.org/html/rfc6143). The add-on `vnc` enables parts
of the rerobots API for worknig with VNC servers in instances. An example with
[HackRF](https://greatscottgadgets.com/hackrf/) hardware together with a window of [GNU Radio](https://www.gnuradio.org/) embedded in a sandbox is
shown in the figure below.

![screenshot of VNC with HackRF](figures/screenshot-vnc-hackrf.jpg)

At this time, the sandbox kind `vnc` is basesd on [noVNC](https://github.com/novnc/noVNC) and requires
instances created from an image derived from Dockerfile-vnc [under the directory
devices/ of the sourcetree](https://github.com/rerobots/hardshare/tree/master/devices).

To add it from the CLI

```bash
hardshare addon-vnc --add
```

and to remove it, `hardshare addon-vnc --rm`.


## VS Code

The add-on `vscode` provides management for [code-server](https://github.com/cdr/code-server) on a per instance
basis. With it, a core [VS Code](https://github.com/Microsoft/vscode) IDE is available as a kind of sandbox.
