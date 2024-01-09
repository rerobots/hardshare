# Tutorial: Seeed Studio XIAO SAMD21

This tutorial begins where [Quickstart](/quickstart) ended and demonstrates
onboarding of a [Seeed Studio XIAO SAMD21](https://wiki.seeedstudio.com/Seeeduino-XIAO/).


## Prerequisites

Before sharing a device via hardshare, check that you can work with it directly.
The main intuition of hardshare is to add reproducibility to a workspace that is
already known to be operational.


## Prepare Docker image

To facilitate efficient reproducibility, we want to prepare a container
image that has requirements already installed so the user can simply
"build-and-flash" without having to wait for additional toolchains to be
downloaded, built, etc.

With this in mind, we will begin with a base image that already has [PlatformIO](https://docs.platformio.org/en/latest/what-is-platformio.html)
installed. Then, we will build example code in a process that will automatically
install required packages for that example. Finally, the resulting container
will be committed as an image that can be re-used.


## Add USB device to hardshare configuration


## Termination scripts

You have the choice of executing a script at the end of every instance.
This script provides automatic clean-up to prepare for the next user,
for example, by erasing memory.


## Stream video

Read more about the `attach-camera` command in the section: [Attach External Camera](/attach_camera).


## Make a sandbox
