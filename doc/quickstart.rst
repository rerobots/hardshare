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


Request instance
----------------

Now that the daemon is active, you can request an instance on your own device
through https://rerobots.net. Go to the URL shown in ::

  hardshare config -l

It will look like
``https://rerobots.net/workspace/b47cd57c-833b-47c1-964d-79e5e6f00dba`` but with
a different ID. After requesting the instance, it should go from ``INIT`` to
``READY``. If something breaks, for example arriving at ``INIT_FAIL`` status,
terminate the instance from https://rerobots.net/instances and then read
:doc:`install` or :doc:`help`. For example, the first ``INIT`` can appear slow
because a Docker image is downloaded, :ref:`as described elsewhere
<ssec:install-preparing-cprovider>`.

Otherwise, congratulations on successful instantiation from your own device!
Terminate the instance from https://rerobots.net/instances and, from the
command-line, stop the daemon::

  hardshare terminate

Now entering ``hardshare status`` should return ``daemon_found: false``.


Check permissions
-----------------

When a new device is registered, the default configuration permits only you
(i.e., your user account) to create instances. To get the list of access rules::

  hardshare rules -l

which should only have 1 item under ``rules``: a capability ``CAP_INSTANTIATE``
and your username. To allow other users::

  hardshare rules --permit-all


What next?
----------

At this stage, you have a ready-to-use hardshare host. Next:

* :doc:`attach_camera` to provide video streams of your hardware.
* Go to https://rerobots.net/hardshare and create your first sandbox.
* If you have a PlatformIO_-compatible board (for example, ESP32_), do the :doc:`tutorial_esp32`.
* If you have a Misty_ robot, do the :doc:`tutorial_misty`.


.. _PlatformIO: https://docs.platformio.org/en/latest/what-is-platformio.html
.. _ESP32: https://docs.espressif.com/projects/esp-idf/en/latest/esp32/
.. _Misty: https://www.mistyrobotics.com/
.. _Ubuntu: https://ubuntu.com/download/desktop
