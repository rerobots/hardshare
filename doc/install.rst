Installation instructions
=========================

Summary
-------

The details vary depending upon the hardware that you want to share. These
instructions are work in progress, and contributions and feedback are welcome.
Please open a ticket at https://github.com/rerobots/hardshare/issues


Building images for Docker
--------------------------

In the directory ``robots`` of the sourcetree, there are Dockerfiles that can be
used to create images for containers that are known to work on some robot
hardware, as described in the respective subdirectories. These can be `browsed
on GitHub <https://github.com/rerobots/hardshare/tree/master/robots>`_.

::

  docker build -t hs.rerobots.net/generic:latest -f Dockerfile .
