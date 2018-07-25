Quickstart
==========

Preliminaries
-------------

First, get ``hardshare``::

  pip install hardshare

If that fails, read the detailed :doc:`install`. If it succeeded, then you
should be able to get the version from the command-line interface (CLI)::

  hardshare version

To prepare your robot hardware for sharing, look for it in the robots/ directory
of the hardshare sourcetree (available for `browsing on GitHub
<https://github.com/rerobots/hardshare/tree/master/robots>`_).

* If you do not already have one, create your user account at https://rerobots.net/
* Create an `API token <https://rerobots.net/tokens>`_ for your hardshare client
  to use. (Learn how to do this `from the rerobots Web Guide
  <https://help.rerobots.net/webui.html#making-and-revoking-api-tokens>`_.)
* hardshare config -c
  hardshare config --add-key path/to/your/jwt.txt
  hardshare register
  hardshare check

Start the daemon::

  hardshare ad -d

Check it::

  hardshare status
