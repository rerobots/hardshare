Maintenance
===========

This page describes common routines for maintaining hardshare configurations and
shared devices. Another page provides detailed discussion about :doc:`bestpractices`.


.. highlight:: none

How to find all hardshare procesess?
------------------------------------

To find all relevant processes::

  ps -AHF | grep -i hardshare

Beware that this can return several processes that include "hardshare" in their
arguments but are not hardshare processes per se.  The left-most number in each
returned row is the PID. These processes can be killed via ``kill`` or
``kill -SIGINT``.


Daemon fails to start or is not responsive
------------------------------------------

::

   hardshare status

::

  - daemon_found: false
    has_instance: false
    provider: podman
    warnings:
    - fail to open daemon socket at /home/scott/.rerobots/hardshare.b47cd57c-833b-47c1-964d-79e5e6f00dba.sock
    wdeployment: b47cd57c-833b-47c1-964d-79e5e6f00dba


Delete the ``.sock`` file shown.


List local configurations
-------------------------

::

   hardshare --format=yaml config -l

::

  local:
    err_keys: {}
    keys:
    - /home/scott/.rerobots/keys/jwt.txt
    ssh_key: /home/scott/.ssh/unodist
    version: 0
    wdeployments:
    - cargs: []
      container_name: rrc
      cprovider: podman
      id: b47cd57c-833b-47c1-964d-79e5e6f00dba
      image: hs-generic
      init_inside: []
      owner: scott
      terminate: []
  remote:
    deployments:
    - date_created: 2020-05-25 06:27 UTC
      id: b47cd57c-833b-47c1-964d-79e5e6f00dba
      origin: null
    owner: scott


Start, check, and stop daemons
------------------------------

::

   hardshare ad -d
   hardshare status
   hardshare terminate


Update API tokens
-----------------

Remove any expired API tokens::

  hardshare config -p

Then, `get a new API token <https://rerobots.net/tokens>`_, and add it::

  hardshare config --add-key path/to/your/jwt.txt
