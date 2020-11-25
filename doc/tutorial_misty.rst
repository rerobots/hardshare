Tutorial: Misty
===============

This tutorial begins where :doc:`quickstart` ended. This tutorial provides
typical preparation for sharing a Misty_ robot via hardshare:

1. configure a proxy to the API of your Misty,
2. attach a camera to the workspace,
3. create a sandbox with example code, as shown in the figure below.

.. image:: figures/screenshot-misty2sandbox-20201124.png


Declare the add-on mistyproxy
-----------------------------

For example, if the Misty robot is available on the local network at IP address
192.168.0.144, then::

  hardshare addon-mistyproxy --add --ip 192.168.0.144


Check details via the rerobots CLI
----------------------------------

You can check the details associated with your Misty robot deployment through
the `rerobots CLI <https://rerobots-py.readthedocs.io/en/latest/cli.html>`_.  If
you do not have it, try ``pip install rerobots`` as described `in the
introduction to the rerobots CLI <https://rerobots-py.readthedocs.io/en/latest/intro.html>`_.

The rerobots CLI requires an API token, much like the hardshare client. To find your locally, ::

  hardshare config -l

and notice the path under the section "found keys". Now call ``rerobots wdinfo``
to get details associated with your workspace deployment; for example, ::

  $ rerobots -t /home/scott/.rerobots/keys/jwt.txt wdinfo 2d6039bc-7c83-4d46-8567-c8df4711c386
  {
    "id": "2d6039bc-7c83-4d46-8567-c8df4711c386",
    "type": "user_provided",
    "type_version": 1,
    "supported_addons": [
      "cmdsh",
      "mistyproxy"
    ],
    "desc": "",
    "region": "",
    "icounter": 33,
    "created": "2020-09-18 05:25:49.711260",
    "queuelen": 0,
    "addons_config": {
      "mistyproxy": {
	"ip": "192.168.0.144"
      }
    },
    "cap": [
      {
	"id": 46,
	"date_created": "2020-10-21 22:24:07.808679",
	"user": "scott",
	"wdeployment_id": "2d6039bc-7c83-4d46-8567-c8df4711c386",
	"capability": "CAP_INSTANTIATE",
	"param": null
      }
    ]
  }


Stream video
------------

For most users, streaming video is most easily achieved with the command
``attach-camera`` as described in the section: :doc:`attach_camera`.

When done, the ``cam`` add-on appears on your workspace deployment, which
practically means that remote users will be able capture camera images or view a
live stream like that at https://rerobots.net/sandbox/fixed_misty2


Make a sandbox
--------------

At this stage, your workspace deployment with a Misty robot is ready for the
kind of low-level access demonstrated in the tutorial at https://help.rerobots.net/tutorial_proxy_fixedmisty.html

To provide a faster and simpler (and also more constrained) experience for
remote users of your Misty, you can make a sandbox.
A similar arrangement is shown in `the following video <https://vimeo.com/440801712>`_:

.. raw:: html

  <iframe src="https://player.vimeo.com/video/440801712" width="640" height="360" frameborder="0" webkitallowfullscreen mozallowfullscreen allowfullscreen></iframe>

Go to your hardshare administrative panel at https://rerobots.net/hardshare
and in the "Sandboxes" section, do "create new".
Select the sandbox kind ``misty-py``.


.. _Misty: https://www.mistyrobotics.com/
