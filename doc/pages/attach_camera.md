---
title: Attach External Camera
---

# Attach External Camera

## Summary

Besides [addons](/addons) like `cam` that stream video via the device within an
instance, hardshare provides for presenting images from a camera not available
inside the instance. Furthermore, rectangular subsets of this image can be shown
for different workspaces.


## Prerequisites

Only supported on Linux and macOS. If you are interested in other host operating
systems, please [contact us](https://rerobots.net/contact).

## Prepare a New Configuration

If you have remote access to the array of devices, then the following process
should suffice to create a new configuration:

1. Capture a full-size image. The shape can be specified with `--width-height` when calling `hardshare attach-camera`.
2. Program each device in turn and watch to determine which device has a screen that clears (indicating that it is being flashed).
3. Open the full image in an image editor and find top-left and bottom-right corners of rectangles for each board.
4. Create the map file, upload it to your host, and start hardshare attach-camera with it.

Note that some steps can be changed if you have physical access. For example,
instead of programming each device in turn, you can physically detach and
re-attach a device to find the file that corresponds to the device (e.g.,
`/dev/ttyUSB0`), while observing its location in the array.

The map file has [JSON](https://www.json.org/json-en.html) syntax and the form: `{"<wd identifier>": [X0, Y0, X1, Y1], ...}`.
For example,

```json
{
    "c9f5e2df-f233-4084-9d01-c7f119e3f9a5": [190, 133, 442, 424],
    "2a789bea-5f46-4dc0-a8a3-bf26759ca329": [500, 500, 800, 800]
}
```

would make the rectangle with corners (190, 133) and (442, 424) available as the
camera feed for workspace deployment c9f5e2df-f233-4084-9d01-c7f119e3f9a5, and
the square with corners (500, 500) and (800, 800) would be for 2a789bea-5f46-4dc0-a8a3-bf26759ca329.

Some cameras require additional tuning, such as turning off auto-focus or
increasing brightness. On Linux hosts, try [v4l-utils](https://www.linuxtv.org/wiki/index.php/V4l-utils). For example,

```bash
v4l2-ctl -d /dev/video2 --all
```

will result in a dump of all information about the camera.  Continuing this
example, a feature like automatic focusing appears as a parameter

```
focus_auto 0x009a090c (bool)   : default=1 value=1
```

Then, to turn it off,

```bash
v4l2-ctl -d /dev/video2 -c focus_auto=0
```


## Attach and Stop Cameras

The contents of a map file is provided to the hardshare daemon via the
command-line argument `--crop`, e.g.,

```bash
hardshare attach-camera --crop $(cat mapfile) 0 c9 2a
```

If `--width-height` was specified earlier, then include it here.
Notice that the above example uses abbreviated references to workspace
deployments. E.g., `c9` abbreviates `c9f5e2df-f233-4084-9d01-c7f119e3f9a5`.
Any unique prefix is acceptable.

The above command will keep the terminal from which you called it busy. To stop
streaming, enter `Ctrl-C` (while holding "Ctrl" button on your keyboard, press
the "c" button).

If you want `attach-camera` to execute as a background process, which would
allow you to then close the terminal window or sign-out of the host without
interrupting video streams, then add `-d`, e.g.,

```bash
hardshare attach-camera -d --crop $(cat mapfile) 0 c9 2a
```

After `hardshare attach-camera -d`, the following command can be used to stop
all `attach-camera` background processes

```bash
hardshare stop-cameras
```
