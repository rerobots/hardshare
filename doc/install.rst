Installation Instructions
=========================

Summary
-------

The goals of this page are to provide more detail than :doc:`quickstart` and to
treat special cases. If :doc:`quickstart` worked for you, then you can safely
skip this page.

The details vary depending upon the hardware that you want to share. These
instructions are work in progress, and contributions and feedback are welcome.
Please open a ticket at https://github.com/rerobots/hardshare/issues

Installation instructions are provided for modern GNU/Linux distributions, such
as Ubuntu_. We are working to support other kinds of hosts, including Windows,
macOS, and FreeBSD.


Main Aspects
------------

The main aspects to an operational ``hardshare`` installation:

1. API token for a rerobots_ user account,
2. ``hardshare`` client,
3. a container provider (also known as *cprovider*),
4. rules around instance initialization, termination, and filtering.

To begin, initialize a new configuration::

  hardshare config -c


.. _ssec:api-tokens:

API Tokens
``````````

Instructions about managing API tokens are `in the rerobots Web Guide
<https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_. The
token that you create at https://rerobots.net/tokens and download is saved to
your local hardshare configuration. As such, the default expiration time might
be too small for your application. Download the token, and add it::

  hardshare config --add-token path/to/your/jwt.txt


Containers
``````````

Hardshare shares hardware among remote users through containers. The term
*container* in the context of hardshare includes Linux containers. Supporting
software that facilitates containers in hardshare are known *cproviders*. For new
users, Docker is a good first cprovider to try and is the default in a
newly installed ``hardshare`` client configuration.

Finally, the primary client is implemented in Python and `available via PyPI
<https://pypi.org/project/hardshare/>`_::

  pip install hardshare

or ``pipenv install hardshare`` if Pipenv_ is installed.
If it succeeded, then you should be able to get the version from the
command-line interface (CLI)::

  hardshare version


.. _ssec:install-preparing-cprovider:

Prepare a cprovider
-------------------

.. _ssec:preparing-docker-cprovider:

Docker
``````

In most cases, Docker images are available via Docker Hub. The correct image to
use depends on your host architecture. On Linux, you can do ::

  uname -m

to find this. For example, on Raspberry Pi this would be ``armv7l``, so Docker
image tags that begin with ``armv7l-`` can be used. To get the latest release of
the base generic image::

  docker pull rerobots/hs-generic:armv7l-latest

which pulls the image from `Docker Hub <https://hub.docker.com/r/rerobots/hs-generic>`_.
To declare this image in the local hardshare configuration::

  hardshare config --assign-image rerobots/hs-generic:armv7l-latest

Many consumer "desktop" and "laptop" computers have the ``x86_64`` architecture,
so the corresponding image is instead ``rerobots/hs-generic:x86_64-latest``.

Images in this registry are defined by Dockerfiles `under the directory robots/
of the sourcetree`_.  To build the image from source files, use the command
given in the comments of the Dockerfile. For example, ::

  docker build -t rerobots/hs-generic:latest -f Dockerfile .


Podman
``````

For many operations, podman_ is a drop-in replacement for ``docker``. To switch
to it with an existing hardshare configuration (created as described above), ::

  hardshare config --cprovider podman

Then, :ref:`the section about Docker <ssec:preparing-docker-cprovider>` can be
followed by replacing ``docker`` with ``podman``.


Include Devices
---------------

For example, ::

  hardshare config --add-raw-device /dev/ttyUSB0


Access Rules
------------

Each robot shared through rerobots_ is subject to access rules about who can do
what with it. These rules are said to define *capabilities*. The decision
sequence for a user ``username`` trying to perform some ``action`` is the
following:

1. if there is a rule about ``action`` explicitly for ``username``, then apply it;
2. else, if there is a rule about  ``action`` that is for a class of users of which ``username`` is a member, then apply it;
3. else, if there is a rule about ``action`` that targets all users (indicated by ``*``), then apply it;
4. else (no match), default to not permit.

The most simple kind of rule is whether or not to allow someone to remotely
access a device. When a new device is registered, a single rule is created that
permits only you (i.e., your user account) to create instances. To get the list
of access rules::

  hardshare rules -l

which should only have 1 item under ``rules``: a capability ``CAP_INSTANTIATE``
and your username. To allow other users::

  hardshare rules --permit-all


What Next?
----------

At this stage, you have a ready-to-use hardshare host. Next:

* :doc:`attach_camera` to provide video streams of your hardware.
* Go to https://rerobots.net/hardshare and create your first sandbox.
* If you have a PlatformIO_-compatible board (for example, ESP32_), do the :doc:`tutorial_esp32`.
* If you have a Misty_ robot, do the :doc:`tutorial_misty`.


.. _rerobots: https://rerobots.net/
.. _under the directory robots/ of the sourcetree: https://github.com/rerobots/hardshare/tree/master/robots
.. _PlatformIO: https://docs.platformio.org/en/latest/what-is-platformio.html
.. _ESP32: https://docs.espressif.com/projects/esp-idf/en/latest/esp32/
.. _Misty: https://www.mistyrobotics.com/
.. _Ubuntu: https://ubuntu.com/download/desktop
.. _podman: https://podman.io/
.. _Pipenv: https://pipenv.pypa.io/
