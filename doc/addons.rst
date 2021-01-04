Add-ons
=======

.. highlight:: none

All workspace deployments provided by rerobots_ and all devices provided by
hardshare users support the core API. Additional features, referred to as
*add-ons*, can also be made available if sufficient hardware requirements are
satisfied.

For example, many workspaces include cameras. The add-on ``cam`` enables working
with cameras via ``api.rerobots.net``. Hardshare provides a feature for sharing
a camera feed among several distinct workspace deployments, as described in the
section: :doc:`attach_camera`.


cmdsh
-----

::

  hardshare addon-cmdsh --add

``hardshare addon-cmdsh --rm``


VNC
---

::

  hardshare addon-vnc --add


``hardshare addon-vnc --rm``


.. _rerobots: https://rerobots.net/
.. _under the directory robots/ of the sourcetree: https://github.com/rerobots/hardshare/tree/master/robots
