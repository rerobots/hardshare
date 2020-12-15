Tutorial: ESP32
===============

This tutorial begins where :doc:`quickstart` ended. Using ESP32-DevKitC_ as an
example, this tutorial provides typical preparation for sharing a
PlatformIO_-compatible board via hardshare:

1. pull or create Docker image with required toolchains,
2. find the device files corresponding to the board interface,
3. decide how to erase or reset the device after each user,
4. attach a camera to the workspace,
5. create a sandbox with example code, as shown in the figure below.

.. image:: figures/screenshot-esp32-20201215.jpg


.. highlight:: none

Prerequisites
-------------

Before sharing a device via hardshare, check that you can work with it directly.
The main intuition of hardshare is to add reproducibility to a workspace that is
already known to be operational. In other words, if you can build code and flash
to a device locally using PlatformIO, then you can allow others to do the same
remotely.

With this in mind, begin by getting an example from the repository at
https://github.com/platformio/platform-espressif32 ::

  git clone https://github.com/platformio/platform-espressif32.git
  cd platform-espressif32/examples/espidf-hello-world
  pio run
  pio run -t nobuild -t upload

If an ESP32 board is connected to your host, then this should result in the
espidf-hello-world program being written to it.  Now observe messages received
via serial from the ESP32::

  pio device monitor

If you cannot build the example code, if the device cannot be written to, or if
nothing is received via serial (indicating the program is not executing or the
serial line is broken), then you must debug it before proceeding with this
hardshare tutorial.


Prepare Docker image
--------------------

:ref:`as described elsewhere <ssec:install-preparing-cprovider>`

get image depending on local arch, or build it yourself from Dockerfile-platformio

start container and copy in your project code

docker commit 4e7 demo


Add USB device to hardshare configuration
-----------------------------------------

For example, connecting a ESP32-DevKitC via USB port to a Linux host results in
the following log messages::

  Dec 15 10:58:58 cero kernel: usb 2-1: new full-speed USB device number 4 using xhci_hcd
  Dec 15 10:58:58 cero kernel: usb 2-1: New USB device found, idVendor=10c4, idProduct=ea60, bcdDevice= 1.00
  Dec 15 10:58:58 cero kernel: usb 2-1: New USB device strings: Mfr=1, Product=2, SerialNumber=3
  Dec 15 10:58:58 cero kernel: usb 2-1: Product: CP2102N USB to UART Bridge Controller
  Dec 15 10:58:58 cero kernel: usb 2-1: Manufacturer: Silicon Labs
  Dec 15 10:58:58 cero kernel: usb 2-1: SerialNumber: 2c7c5215b469e91198c86ad63ca6d3af
  Dec 15 10:58:58 cero mtp-probe[12521]: checking bus 2, device 4: "/sys/devices/pci0000:00/0000:00:14.0/usb2/2-1"
  Dec 15 10:58:58 cero mtp-probe[12521]: bus: 2, device: 4 was not an MTP device
  Dec 15 10:58:58 cero kernel: usbcore: registered new interface driver cp210x
  Dec 15 10:58:58 cero kernel: usbserial: USB Serial support registered for cp210x
  Dec 15 10:58:58 cero kernel: cp210x 2-1:1.0: cp210x converter detected
  Dec 15 10:58:58 cero kernel: usb 2-1: cp210x converter now attached to ttyUSB0
  Dec 15 10:58:58 cero mtp-probe[12527]: checking bus 2, device 4: "/sys/devices/pci0000:00/0000:00:14.0/usb2/2-1"
  Dec 15 10:58:58 cero mtp-probe[12527]: bus: 2, device: 4 was not an MTP device


Disconnecting the ESP32 board leads to the following logs::

  Dec 15 10:59:17 cero kernel: usb 2-1: USB disconnect, device number 4
  Dec 15 10:59:17 cero kernel: cp210x ttyUSB0: cp210x converter now disconnected from ttyUSB0
  Dec 15 10:59:17 cero kernel: cp210x 2-1:1.0: device disconnected


The critical information in the above logs is that the ESP32-DevKitC is
associated with the device file ``/dev/ttyUSB0``. Accordingly, the device is
added to the local hardshare configuration::

  hardshare config --add-raw-device /dev/ttyUSB0


On security
-----------

iptables to prevent access of LAN
other constraints for container, e.g., --memory


Termination scripts
-------------------

clean-up (termination script)


Stream video
------------

For most users, streaming video is most easily achieved with the command
``attach-camera`` as described in the section: :doc:`attach_camera`. After
following those instructions, typically it suffices to::

  hardshare attach-camera -d 0

to enable video streaming. Note that the camera is only active when there is an
active instance with your hardshare client. Thus, the above command can succeed
without you noticing any activity on your camera.


Make a sandbox
--------------



.. _Dockerfile-platformio: https://github.com/rerobots/hardshare/tree/master/robots/generic/Dockerfile-platformio
.. _PlatformIO: https://docs.platformio.org/en/latest/what-is-platformio.html
.. _ESP32-DevKitC: https://docs.espressif.com/projects/esp-idf/en/latest/esp32/hw-reference/esp32/get-started-devkitc.html
