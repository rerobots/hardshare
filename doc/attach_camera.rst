Attach external camera
======================

Summary
-------

Besides :doc:`addons` like ``cam`` that stream video via the device within an
instance, hardshare provides for presenting images from a camera not available
inside the instance. Furthermore, rectangular subsets of this image can be shown
for different workspaces.


Prerequisites
-------------

`install OpenCV <https://docs.opencv.org/4.4.0/d2/de6/tutorial_py_setup_in_ubuntu.html>`_


Preparing a new configuration
-----------------------------

1. Capture a full image.
2. Program each device in turn and watch to determine which device has a screen that clears (indicating that it is being flashed).
3. Open the full image in an image editor and find top-left and bottom-right corners of rectangles for each board.
4. Create the map file, upload it to your host, and start hardshare attach-camera with it.

The map file has JSON_ syntax and the form: ``{"<wd identifier>": [X0, Y0, X1, Y1], ...}``.
For example, ::

  {
      "c9f5e2df-f233-4084-9d01-c7f119e3f9a5": [190, 133, 442, 424],
      "2a789bea-5f46-4dc0-a8a3-bf26759ca329": [500, 500, 800, 800],
  }

would make the rectangle with corners (190, 133) and (442, 424) available as the
camera feed for workspace deployment c9f5e2df-f233-4084-9d01-c7f119e3f9a5, and
the square with corners (500, 500) and (800, 800) would be for 2a789bea-5f46-4dc0-a8a3-bf26759ca329.

The contents of a map file is provided to the hardshare daemon via the
command-line argument ``--crop``, e.g., ::

  hardshare attach-camera --crop $(cat mapfile) 0 c9 2a

Notice that the above example uses abbreviated references to workspace
deployments. E.g., ``c9`` abbreviates ``c9f5e2df-f233-4084-9d01-c7f119e3f9a5``.
Any unique prefix is acceptable.


.. _JSON: https://www.json.org/json-en.html
