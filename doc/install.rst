Installation instructions
=========================

Summary
-------

The details vary depending upon the hardware that you want to share. These
instructions are work in progress, and contributions and feedback are welcome.
Please open a ticket at https://github.com/rerobots/hardshare/issues

Installation instructions are provided for modern GNU/Linux distributions, such
as Ubuntu_. We are working to support other kinds of hosts, including Windows,
macOS, and FreeBSD.


Main aspects
------------

There are 3 main aspects to an operational ``hardshare`` installation:

1. API token for a rerobots_ user account,
2. a container provider (also known as *cprovider*),
3. ``hardshare`` client.

Instructions about getting an API token are `in the rerobots Web Guide
<https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_.

Hardshare shares hardware among remote users through containers. The term
*container* in the context of hardshare includes Linux containers. Supporting
software that facilitates containers in hardshare are known *cproviders*. For new
users, Docker is a good first cprovider to try and is the default in a
newly installed ``hardshare`` client configuration.

Finally, the primary client is implemented in Python and `available via PyPI
<https://pypi.org/project/hardshare/>`_::

  pip install hardshare


Preparing for Docker as a cprovider
-----------------------------------

In most cases, Docker images are available via Docker Hub. For example, ::

  docker pull rerobots/hs-generic

which pulls the image from `Docker Hub <https://hub.docker.com/r/rerobots/hs-generic>`_.

This image and others are defined by Dockerfiles in the directory ``robots`` of
the sourcetree. These can be `browsed on GitHub
<https://github.com/rerobots/hardshare/tree/master/robots>`_. To build the image from source files::

  docker build -t hs.rerobots.net/generic:latest -f Dockerfile .


.. _rerobots: https://rerobots.net/
.. _Ubuntu: https://ubuntu.com/download/desktop
