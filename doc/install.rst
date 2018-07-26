Installation instructions
=========================


Building images for Docker
--------------------------

In the directory ``robots`` of the sourcetree, there are Dockerfiles that can be
used to create images for Docker containers that are known to work on some robot
hardware, as described in the respective subdirectories.

::

  docker build -t hs.rerobots.net/generic:latest -f Dockerfile .
