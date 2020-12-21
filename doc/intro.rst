Introduction
============

This page introduces concepts and technical aspects of **hardshare**. If you
just want to get started, skip to the :doc:`quickstart`.


Overview
--------

.. image:: figures/hardshare-overview.svg

As illustrated above, the main parts are:

1. your robot (also referred to as "device" or "devices"),
2. host computer on which a hardshare client runs,
3. rerobots infrastructure,
4. remote users.

The word "robot" is broadly interpreted here: essentially, it is a device or
collection of devices that are connected somehow. The robot and the surrounding
environment are together known as a `workspace deployment
<https://help.rerobots.net/prelim.html>`_. Robots that are owned and managed by
the company rerobots are always presented as workspace deployments with unique
IDs.  To share your robot via hardshare, it also must be assigned a unique ID.

The "rerobots infrastructure" is stuff described at https://help.rerobots.net/
and includes facilities like `a Python client library
<https://rerobots-py.readthedocs.io/en/latest/>`_ and sandboxes_.

The hardshare client is the part that manages the lifecycle of remote access,
including advertising that the robot is available, creating SSH tunnels to a
local container, and enforcing constraints like input filters.

When you create a workspace deployment, you become the owner and can make
adminstrative decisions.

Remote users do not necessarily have rerobots accounts. The kinds of access that
are possible depend on the permissions assigned by the owner.


Lifecycle of instances
----------------------

The time during which a user has exclusive access to a workspace deployment is
known as an `instance <https://help.rerobots.net/prelim.html>`_.  The process of
requesting, getting credentials, and terminating an instance is similar to how
you might get a "compute node" from a "cloud computing" company, like Packet_ or
DigitalOcean_:

1. Someone requests access using the unique ID of the workspace deployment.
2. The remote user is connected through a container that you host locally.
3. Their input/output can be constrained according to filter rules. For example,
   the "reboot" command is dropped, while getting sensor data is accepted.
4. The instance is terminated when the remote user is done. (If needed, you can
   force termination at any time.)


.. image:: figures/instance-lifecycle.svg


.. _Packet: https://www.packet.com/
.. _DigitalOcean: https://www.digitalocean.com/
.. _sandboxes: https://rerobots.net/sandbox


Interfaces around instances
---------------------------

.. image:: figures/layers-illustration.svg

The rerobots/hardshare architecture provides for specifying how hardware appears
to an instance. If carefully configured, any device can be shared through
instances safely and securely. The precise meanings of *safe* and *secure*
depend on the hardware in the workspace deployment, but the basic organization
is the same: associate actions with lifecycle events (e.g., initialization), and
monitor input and output streams.

The default configuration of a new hardshare client installation does not
enforce any contracts. Instances in this case have unfiltered access to
hardware. This can be a good first choice in trusted settings, such as a team
working closely together at a company.
