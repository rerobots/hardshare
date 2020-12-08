Tutorial: Misty
===============

This tutorial begins where :doc:`quickstart` ended. This tutorial provides
typical preparation for sharing a Misty_ robot via hardshare:

1. configure a proxy to the API of your Misty,
2. attach a camera to the workspace,
3. create a sandbox with example code, as shown in the figure below.

.. image:: figures/screenshot-misty2sandbox-20201124.png


.. highlight:: none

Declare the add-on mistyproxy
-----------------------------

For example, if the Misty robot is available on the local network at IP address
192.168.0.144, then::

  hardshare addon-mistyproxy --add --ip 192.168.0.144

To use a different address, the previously saved value must be removed. For
example, to change to 192.168.1.10::

  hardshare addon-mistyproxy --rm
  hardshare addon-mistyproxy --add --ip 192.168.1.10


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
``attach-camera`` as described in the section: :doc:`attach_camera`. After
following those instructions, typically it suffices to::

  hardshare attach-camera -d 0

to enable video streaming. Note that the camera is only active when there is an
active instance with your hardshare client. In other words, the above command
can succeed without you noticing any activity on your camera.

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

Alternatively, the sandbox can present a Blockly_ editor. To do this, select the
sandbox kind ``misty-blockly``. An example is shown in the video at
https://vimeo.com/488264658


Hosting more than 1 Misty
-------------------------

To host more than 1 Misty robot, you must register additional workspace
deployments: each ID should correspond to precisely 1 Misty::

  hardshare register --permit-more

Now ``hardshare config -l`` will show a list of multiple IDs. All commands given
previously in this tutorial and in :doc:`quickstart` can be used again with the
new ID by specifying it on the command-line.

For example, if the new ID returned from the ``hardshare register`` command
above is ``2d6039bc-7c83-4d46-8567-c8df4711c387`` and if IP address of the
second Misty robot is ``192.168.1.11``, then::

  hardshare addon-mistyproxy --add --ip 192.168.1.11 2d6039bc

will declare the address. Notice that the above command has a prefix of the ID
``2d6039bc-7c83-4d46-8567-c8df4711c387``. For convenience, most ``hardshare``
commands accept a unique prefix where a workspace deployment ID is required.


.. _Blockly: https://developers.google.com/blockly/
.. _Misty: https://www.mistyrobotics.com/
