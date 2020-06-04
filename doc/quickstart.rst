Quickstart
==========

To get up and running with **hardshare**, try the instructions below.

For more details, including alternative configurations and special cases, read
:doc:`install`. To learn basic concepts about **hardshare**, read the
:doc:`intro`.


Preliminaries
-------------

First, get ``hardshare``::

  pip install hardshare

If it succeeded, then you should be able to get the version from the
command-line interface (CLI)::

  hardshare version

A container provider must be installed before ``hardshare`` can create
instances. The default is `Docker <https://www.docker.com/get-started>`_. To
check that you have it::

  docker version

Create an `API token <https://rerobots.net/tokens>`_ for your hardshare client
to use. (Learn how to do this `from the rerobots Web Guide
<https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_.)

* hardshare config -c
  hardshare config --add-key path/to/your/jwt.txt
  hardshare config --add-ssh-path path/to/your/ssh_key (e.g., ~/.ssh/id_rsa)
  hardshare register
  hardshare check

Start the daemon::

  hardshare ad -d

Check it::

  hardshare status
