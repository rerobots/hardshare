# Copyright (C) 2020 rerobots, Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""Hardshare add-ons

This is free software, released under the Apache License, Version 2.0.
You may obtain a copy of the License at https://www.apache.org/licenses/LICENSE-2.0


SCL <scott@rerobots.net>
Copyright (C) 2020 rerobots, Inc.
"""
import asyncio
import base64
from glob import glob
from io import BytesIO
import logging
import os
import os.path
import signal
import sys
import time

try:
    from asyncio import get_running_loop
except ImportError:
    from asyncio import get_event_loop as get_running_loop

import aiohttp

# inline import:
#   cv2
#   PIL

from .mgmt import get_local_config


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

        except KeyboardInterrupt:
            active = False

        except Exception as err:
            logger.error('caught {}: {}'.format(type(err), err))

        finally:
            await session.close()


async def register_camera_uploader(opts, tok):
    headers = {'Authorization': 'Bearer {}'.format(tok)}
    async with aiohttp.ClientSession(headers=headers) as session:
        async with session.post('https://api.rerobots.net/hardshare/cam', json=opts) as res:
            assert res.status == 200
            return (await res.json())['id']


async def unregister_camera_uploader(hscamera_id, tok):
    headers = {'Authorization': 'Bearer {}'.format(tok)}
    async with aiohttp.ClientSession(headers=headers) as session:
        await session.delete('https://api.rerobots.net/hardshare/cam/{}'.format(hscamera_id))


async def unregister_camera_uploaders(config, tok, allcam=False):
    if not allcam:
        wdeployments = set([x['id'] for x in config['wdeployments']])
    headers = {'Authorization': 'Bearer {}'.format(tok)}
    async with aiohttp.ClientSession(headers=headers) as session:
        stopped_via_pids = []
        for pid_file in glob(os.path.join(os.path.expanduser('~'), '.rerobots', 'cam.*.pid')):
            os.kill(int(open(pid_file).read()), signal.SIGINT)
            _, hscamera_id, _ = os.path.basename(pid_file).split('.')
            stopped_via_pids.append(hscamera_id)
            os.unlink(pid_file)

        res = await session.get('https://api.rerobots.net/hardshare/cam')
        assert res.status == 200
        payload = await res.json()
        for hscamera_id, assoc in payload.items():
            if not allcam and (hscamera_id in stopped_via_pids):
                continue
            if not allcam and len(wdeployments.intersection(set(assoc))) == 0:
                continue
            res = await session.delete('https://api.rerobots.net/hardshare/cam/{}'.format(hscamera_id))
            assert res.status == 200


def stop_cameras(tok, allcam=False):
    config = get_local_config()
    loop = asyncio.get_event_loop()
    try:
        loop.run_until_complete(unregister_camera_uploaders(config, tok, allcam=allcam))
    except KeyboardInterrupt:
        pass


def camera_main(wdeployments, tok, dev, rotate=None, width=None, height=None, crop=None):
    opts = {'wds': wdeployments}
    if crop:
        opts['crop'] = crop
    loop = asyncio.get_event_loop()
    hscamera_id = loop.run_until_complete(register_camera_uploader(opts, tok))
    pid_file = os.path.join(os.path.expanduser('~'), '.rerobots', 'cam.{}.pid'.format(hscamera_id))
    with open(pid_file, 'wt') as fp:
        fp.write(str(os.getpid()))
    try:
        loop.run_until_complete(camera_upload(hscamera_id, dev, tok, rotate, width, height))
    except KeyboardInterrupt:
        pass
    finally:
        loop.run_until_complete(unregister_camera_uploader(hscamera_id, tok))
        try:
            os.unlink(pid_file)
        except OSError:
            pass  # Assume deleted by other process, e.g., unregister_camera_uploaders()


async def __update_supported_async(wdeployment_id, tok, addon, config=None, rm=False):
    headers = {'Authorization': 'Bearer {}'.format(tok)}
    async with aiohttp.ClientSession(headers=headers) as session:
        res = await session.get('https://api.rerobots.net/deployment/{}'.format(wdeployment_id))
        assert res.status == 200
        payload = await res.json()
        update_payload = {
            'supported_addons': payload['supported_addons'],
        }
        if rm and (addon not in payload['supported_addons']):
                return
        elif (not rm) and (addon in payload['supported_addons']):
                return
        if 'addons_config' in payload:
            update_payload['addons_config'] = payload['addons_config']
            if rm and addon in update_payload['addons_config']:
                del update_payload['addons_config'][addon]
        if rm:
            update_payload['supported_addons'].remove(addon)
        else:
            update_payload['supported_addons'].append(addon)
            if config is not None:
                if 'addons_config' not in update_payload:
                    update_payload['addons_config'] = dict()
                update_payload['addons_config'][addon] = config
        res = await session.post('https://hs.rerobots.net/wd/{}'.format(wdeployment_id), json=update_payload)
        assert res.status == 200


def add_cmdsh(wdeployment_id, tok):
    loop = asyncio.get_event_loop()
    try:
        loop.run_until_complete(__update_supported_async(wdeployment_id, tok, addon='cmdsh'))
    except KeyboardInterrupt:
        pass

def rm_cmdsh(wdeployment_id, tok):
    loop = asyncio.get_event_loop()
    try:
        loop.run_until_complete(__update_supported_async(wdeployment_id, tok, addon='cmdsh', rm=True))
    except KeyboardInterrupt:
        pass


def add_mistyproxy(wdeployment_id, tok, targetaddr):
    loop = asyncio.get_event_loop()
    try:
        loop.run_until_complete(__update_supported_async(wdeployment_id, tok, addon='mistyproxy', config={'ip': targetaddr}))
    except KeyboardInterrupt:
        pass

def rm_mistyproxy(wdeployment_id, tok):
    loop = asyncio.get_event_loop()
    try:
        loop.run_until_complete(__update_supported_async(wdeployment_id, tok, addon='mistyproxy', rm=True))
    except KeyboardInterrupt:
        pass
