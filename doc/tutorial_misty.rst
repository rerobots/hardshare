Tutorial: Misty
===============

This tutorial begins where :doc:`quickstart` ended. This tutorial provides
typical preparation for sharing a Misty_ robot via hardshare.


Declaring the add-on mistyproxy
-------------------------------

For example, if the Misty robot is available on the local network at IP address
192.168.0.144, then::

  hardshare addon-mistyproxy --add --ip 192.168.0.144


Checking details via the rerobots CLI
-------------------------------------

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


.. _Misty: https://www.mistyrobotics.com/
