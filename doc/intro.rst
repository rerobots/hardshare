Introduction
============

This page introduces concepts and technical aspects of **hardshare**. If you
just want to get started, skip to the :doc:`quickstart`.


Overview
--------

.. image:: figures/hardshare-overview.svg

With **hardshare**, you can make your devices available to remote users through
reproducible instances. From the perspective of remote users, the basic pattern
is similar to that of "cloud computing" companies like Packet_ or DigitalOcean_:

1. Your device (or "robot") is given a unique ID.
2. Someone requests access using the unique ID.
3. The remote user is connected through a container that you host locally to the
   device.
4. Their input/output can be constrained according to filter rules. For example,
   the "reboot" command is dropped, while getting sensor data is accepted.
5. The instance is terminated when the remote user is done. (If needed, you can
   force termination at any time.)

A device (or "robot") is made available for sharing by presenting it as part of
a `workspace deployment <https://help.rerobots.net/prelim.html>`_


.. _Packet: https://www.packet.com/
.. _DigitalOcean: https://www.digitalocean.com/
