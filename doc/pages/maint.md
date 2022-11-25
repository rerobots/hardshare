Maintenance
===========

This page describes common routines for maintaining hardshare configurations and
shared devices. Another page provides detailed discussion about :doc:`bestpractices`.


.. highlight:: none

How to find all hardshare processes?
------------------------------------

To find all relevant processes::

  ps -AHF | grep -i hardshare

Beware that this can return several processes that include "hardshare" in their
arguments but are not hardshare processes per se.  The left-most number in each
returned row is the PID. These processes can be killed via ``kill`` or
``kill -SIGINT``.


After first installation, instance status INIT_FAIL
---------------------------------------------------

There are many reasons why an instance can fail to initialize, depending on your
configuration. For a newly configured hardshare installation that uses Docker,
first check that the Docker image is compatible with your host architecture. To
do this, first ::

  hardshare config -l

and find the Docker image line; for example, ::

  cprovider: docker
  cargs: []
  img: rerobots/hs-generic

indicates the image rerobots/hs-generic:latest ("latest" is implied if not
present). Now, get your host architecture as known to Linux::

  # uname -m
  x86_64

The output might be different, such as ``armv7l`` on some Raspyberry Pi
boards. Continuing the example above, we can pull the base generic Docker image
for x86_64 hosts::

  docker image pull rerobots/hs-generic:x86_64-latest

and update the hardshare configuration with the tag name::

  hardshare config --assign-image rerobots/hs-generic:x86_64-latest

Now restart the hardshare daemon::

  hardshare stop-ad
  hardshare ad -d

Finally, request an instance as usual.


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
    err_api_tokens: {}
    api_tokens:
    - /home/scott/.rerobots/tokens/jwt.txt
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
   hardshare stop-ad


Update API tokens
-----------------

Remove any expired API tokens::

  hardshare config -p

Then, `get a new API token <https://rerobots.net/tokens>`_, and add it::

  hardshare config --add-token path/to/your/jwt.txt
