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

Instructions about managing API tokens are `in the rerobots Web Guide
<https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_. The
token that you create at https://rerobots.net/tokens and download is saved to
your local hardshare configuration. As such, the default expiration time might
be too small for your application.

There might already be an SSH key at ``~/.ssh/id_rsa``. If not, or if you want to
create a new pair for this purpose, then::

  ssh-keygen

to start an interactive process to create a new pair. The default options are
sufficient here; the prompt "default" is selected by simply pushing ENTER
without typing text. Additional instructions about creating and working with SSH
keys, for example from `DigitalOcean
<https://www.digitalocean.com/community/tutorials/how-to-set-up-ssh-keys--2>`_
or `GitHub
<https://help.github.com/en/github/authenticating-to-github/connecting-to-github-with-ssh>`_.

Hardshare shares hardware among remote users through containers. The term
*container* in the context of hardshare includes Linux containers. Supporting
software that facilitates containers in hardshare are known *cproviders*. For new
users, Docker is a good first cprovider to try and is the default in a
newly installed ``hardshare`` client configuration.

Finally, the primary client is implemented in Python and `available via PyPI
<https://pypi.org/project/hardshare/>`_::

  pip install hardshare

or ``pipenv install hardshare`` if Pipenv_ is installed.


Preparing a cprovider
---------------------

Docker
``````

In most cases, Docker images are available via Docker Hub. For example, ::

  docker pull rerobots/hs-generic

which pulls the image from `Docker Hub <https://hub.docker.com/r/rerobots/hs-generic>`_.

This image and others are defined by Dockerfiles in the directory ``robots`` of
the sourcetree. These can be `browsed on GitHub
<https://github.com/rerobots/hardshare/tree/master/robots>`_. To build the image from source files::

  docker build -t hs.rerobots.net/generic:latest -f Dockerfile .


Podman
``````

For many operations, podman_ is a drop-in replacement for ``docker``. To switch
to it with an existing hardshare configuration (created as described above),
find the ``cprovider`` entry in the configuration file at ``~/.rerobots/main``,
and change it to ``podman``.


Including devices
-----------------



.. _rerobots: https://rerobots.net/
.. _Ubuntu: https://ubuntu.com/download/desktop
.. _podman: https://podman.io/
.. _Pipenv: https://pipenv.pypa.io/
