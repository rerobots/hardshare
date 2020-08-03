Attach external camera
======================

Summary
-------

Besides :doc:`addons` like ``cam`` that stream video via the device within an
instance, hardshare provides for presenting images from a camera not available
inside the instance. Furthermore, rectangular subsets of this image can be shown
for different workspaces.


Preparing a new configuration
-----------------------------

1. Capture a full image.
2. Program each device in turn and watch to determine which device has a screen that clears (indicating that it is being flashed).
3. Open the full image in an image editor and find top-left and bottom-right corners of rectangles for each board.
4. Create the map file, upload it to your host, and start hardshare attach-camera with it.
