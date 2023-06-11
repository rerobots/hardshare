---
title: Introduction
---

# Introduction

This page introduces concepts and technical aspects of **hardshare**. If you
just want to get started, skip to the [Quickstart](/quickstart).


## Overview

![overview](figures/hardshare-overview.svg)

As illustrated above, the main parts are:

1. your robot (also referred to as "device" or "devices"),
2. host computer on which a hardshare client runs,
3. rerobots infrastructure,
4. remote users.

The robot and the surrounding environment are together known as a [workspace
deployment](https://docs.rerobots.net/intro). Robots that are owned and managed
by the company rerobots are always presented as workspace deployments with
unique IDs.  To share your robot via hardshare, it also must be assigned a
unique ID.

In hardshare, the terms *robot* and *device* are interchangeable. Minimally, it
is some hardware with output or input. A workspace can have multiple devices (or
multiple robots).

The "rerobots infrastructure" is stuff described at [docs.rerobots.net](https://docs.rerobots.net/)
and includes facilities like [a Python client library](
https://pypi.org/project/rerobots) and [sandboxes](
https://rerobots.net/sandbox).

The hardshare client is the part that manages the lifecycle of remote access,
including advertising that the robot is available, creating SSH tunnels to a
local container, and enforcing constraints like input filters.

When you create a workspace deployment, you become the owner and can make
adminstrative decisions.

Remote users do not necessarily have rerobots accounts. The kinds of access that
are possible depend on the permissions assigned by the owner.


## Lifecycle of Instances

The time during which a user has exclusive access to a workspace deployment is
known as an [instance](https://docs.rerobots.net/intro).  The process of
requesting, getting credentials, and terminating an instance is similar to how
you might get a "compute node" from a "cloud computing" company:

1. Someone requests access using the unique ID of the workspace deployment.
2. The remote user is connected through a container that you host locally.
3. Their input/output can be constrained according to filter rules. For example,
   the "reboot" command is dropped, while getting sensor data is accepted.
4. The instance is terminated when the remote user is done. (If needed, you can
   force termination at any time.)

The lifecycle of instances is illustrated below. In summary, every instance
begins at `INIT`, can be used while `READY`. If there is an error during
initialization, the instance is marked as `INIT_FAIL`. While `READY`, the
instance can be terminated (that is, permanently stopped), either automatically
when it expires or manually by the user or owner.

For details, read the [rerobots introduction](https://docs.rerobots.net/intro).

{% image src="figures/instance-lifecycle.svg" alt="diagram of instance lifecycle" maxWidth="400px" /%}


## Interfaces Around Instances

{% image src="figures/layers-illustration.svg" alt="layers" maxWidth="400px" /%}

The rerobots/hardshare architecture provides for specifying how hardware appears
to an instance. If carefully configured, any device can be shared through
instances safely and securely. The precise meanings of *safe* and *secure*
depend on the hardware in the workspace deployment, but the basic organization
is the same: associate actions with lifecycle events (e.g., initialization), and
filter input and output streams.

The default configuration of a new hardshare client installation does not
enforce any contracts. Instances in this case have unfiltered access to
hardware. This can be a good first choice in trusted settings, such as a team
working closely together at the same company.
