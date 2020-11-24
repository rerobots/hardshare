Installation instructions
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


Main aspects
------------

The main aspects to an operational ``hardshare`` installation:

1. API token for a rerobots_ user account,
2. an SSH key pair,
3. ``hardshare`` client,
4. a container provider (also known as *cprovider*),
5. rules around instance initialization, termination, and filtering.

To begin, initialize a new configuration::

  hardshare config -c


.. _ssec:api-tokens:

API tokens
``````````

Instructions about managing API tokens are `in the rerobots Web Guide
<https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_. The
token that you create at https://rerobots.net/tokens and download is saved to
your local hardshare configuration. As such, the default expiration time might
be too small for your application. Download the token, and add it::

  hardshare config --add-key path/to/your/jwt.txt


SSH keys
````````

An SSH key is required to create SSH tunnels through which remote users connect
to containers that you host. This section describes how to manually create keys
and some security considerations. Because a key pair is created as part of a new
configuration (``hardshare config -c``) automatically, this section can be
skipped unless something breaks.

There might already be an SSH key at ``~/.ssh/id_rsa``. If not, or if you want to
create a new pair for this purpose, then::

  ssh-keygen

to start an interactive process to create a new pair. The default options are
sufficient here; the prompt "default" is selected by simply pushing ENTER
without typing text. For convenience, we recommend that you do not create a
password for the key. If you insist, then managing such a key is discussed
below. Additional instructions about creating and working with SSH keys, for
example from DigitalOcean_ or GitHub_.

The SSH key is used by the hardshare client in a way that does not motivate
adding password protection: to create reverse tunnels from rerobots-managed
servers into containers that you host. Only the public key is copied to the
rerobots server-side. Furthermore, :ref:`API tokens <ssec:api-tokens>` provide
for authentication and authorization of the hardshare client with respect to
your rerobots account. In summary, this SSH key has a technical role and
provides for encryption, but exposure risk of the secret key small.

If the SSH key has a password, then there must be some way for the hardshare
client to use the key without having to know the password. For this, you should
configure ``ssh-agent``, usage of which is presented in the `OpenBSD manual`_.
If you are new to ``ssh-agent``, we recommend reading about basic ideas of how
it works at http://www.unixwiz.net/techtips/ssh-agent-forwarding.html

Finally, add the SSH secret key path::

  hardshare config --add-ssh-path path/to/your/ssh_key


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


Include devices
---------------

For example, ::

  hardshare config --add-raw-device /dev/ttyUSB0


Access rules
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


What next?
----------

At this stage, you have a ready-to-use hardshare host. Next:

* :doc:`attach_camera` to provide video streams of your hardware.
* Go to https://rerobots.net/hardshare and create your first sandbox.
* If you have a Misty_ robot, do the :doc:`tutorial_misty`.


.. _rerobots: https://rerobots.net/
.. _under the directory robots/ of the sourcetree: https://github.com/rerobots/hardshare/tree/master/robots
.. _Misty: https://www.mistyrobotics.com/
.. _Ubuntu: https://ubuntu.com/download/desktop
.. _podman: https://podman.io/
.. _Pipenv: https://pipenv.pypa.io/
.. _DigitalOcean: https://www.digitalocean.com/community/tutorials/how-to-set-up-ssh-keys--2
.. _GitHub: https://help.github.com/en/github/authenticating-to-github/connecting-to-github-with-ssh
.. _OpenBSD manual: http://man.openbsd.org/OpenBSD-current/man1/ssh-agent.1
