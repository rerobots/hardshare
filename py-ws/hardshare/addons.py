"""Hardshare add-ons

This is free software, released under the Apache License, Version 2.0.
You may obtain a copy of the License at https://www.apache.org/licenses/LICENSE-2.0


SCL <scott@rerobots.net>
Copyright (C) 2020 rerobots, Inc.
"""
import asyncio
import base64
from io import BytesIO
import logging
import sys
import time

try:
    from asyncio import get_running_loop
except ImportError:
    from asyncio import get_event_loop as get_running_loop

import aiohttp
import requests

# inline import:
#   cv2
#   PIL


logger = logging.getLogger(__name__)


async def camera_upload(hscamera_id, dev, tok, rotate, width, height):
    import cv2
    from PIL import Image

    ENCODING_PREFIX = 'data:image/jpeg;base64,'

    headers = {'Authorization': 'Bearer {}'.format(tok)}
    uri = 'https://api.rerobots.net/hardshare/cam/{}/upload'.format(hscamera_id)
    active = True
    while active:
        session = aiohttp.ClientSession(headers=headers)
        try:
            async with session.ws_connect(uri, timeout=30.0, autoping=True) as ws:
                cam = cv2.VideoCapture(dev)
                adjusted = False if width is not None else True

                timeout = 3
                switched = False
                st = time.time()
                while True:
                    if not cam.isOpened():
                        if not switched and (time.time() - st >= timeout):
                            switched = True
                            cam = cv2.VideoCapture('/dev/video{}'.format(dev))
                        await asyncio.sleep(1)
                        continue
                    if not adjusted and width and cam.get(cv2.CAP_PROP_FRAME_HEIGHT) != height:
                        if cam.set(cv2.CAP_PROP_FRAME_WIDTH, width) or cam.set(cv2.CAP_PROP_FRAME_HEIGHT, height):
                            adjusted = True
                    ret, frame = cam.read()
                    img = Image.fromarray(frame[:,:,[2,1,0]], mode='RGB')
                    if rotate == 270:
                        img = img.transpose(Image.ROTATE_270)
                    elif rotate == 90:
                        img = img.transpose(Image.ROTATE_90)
                    elif rotate == 180:
                        img = img.transpose(Image.ROTATE_180)
                    elif rotate == 0:
                        pass
                    elif rotate is not None:
                        pass  # TODO: general case
                    buf = BytesIO()
                    img.save(buf, 'JPEG')
                    await ws.send_str(ENCODING_PREFIX + base64.b64encode(buf.getvalue()).decode('utf-8'))
                    await asyncio.sleep(0.1)

        except asyncio.CancelledError:
            active = False

        except Exception as err:
            logger.error('caught {}: {}'.format(type(err), err))

        finally:
            await session.close()


def camera_main(wdeployments, tok, dev, rotate=None, width=None, height=None, crop=None):
    headers = {'Authorization': 'Bearer {}'.format(tok)}
    opts = {'wds': wdeployments}
    if crop:
        opts['crop'] = crop
    res = requests.post('https://api.rerobots.net/hardshare/cam', json=opts, headers=headers)
    assert res.ok
    hscamera_id = res.json()['id']
    loop = asyncio.get_event_loop()
    try:
        loop.run_until_complete(camera_upload(hscamera_id, dev, tok, rotate, width, height))
    except KeyboardInterrupt:
        pass
    finally:
        res = requests.delete('https://api.rerobots.net/hardshare/cam/{}'.format(hscamera_id), headers=headers)
        assert res.ok
