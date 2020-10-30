Quickstart
==========

To get up and running with **hardshare**, try the instructions below.

For more details, including alternative configurations and special cases, read
:doc:`install`. To learn basic concepts about **hardshare**, read the
:doc:`intro`.


.. highlight:: none

Preliminaries
-------------

Instuctions below are known to work on modern GNU/Linux distributions, for
example Ubuntu_.

First, get ``hardshare``::

  pip install hardshare

If it succeeded, then you should be able to get the version from the
command-line interface (CLI)::

  hardshare version

A container provider must be installed before ``hardshare`` can create
instances. The default is `Docker <https://www.docker.com/get-started>`_. To
check that you have it::

  docker version


Create local configuration
--------------------------

Get an `API token <https://rerobots.net/tokens>`_ to associate with your
hardshare client. (Learn how to do this `from the rerobots Web Guide
<https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_.) Then,
initialize a new local configuration, and add the API token to it::

  hardshare config -c
  hardshare config --add-key path/to/your/jwt.txt

Finally, create a new workspace deployment::

  hardshare register


Start sharing!
--------------

Start the daemon::

  hardshare ad -d

Check it::

  hardshare status

which should result in output that is similar to the following::

  - daemon_found: true
    has_instance: false
    provider: docker
    wdeployment: b47cd57c-833b-47c1-964d-79e5e6f00dba


.. _Ubuntu: https://ubuntu.com/download/desktop
