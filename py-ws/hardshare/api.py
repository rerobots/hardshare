#!/usr/bin/env python
# Copyright (C) 2018 rerobots, Inc.
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
"""hardshare API client
"""
import asyncio
from concurrent.futures import FIRST_COMPLETED
import json
import os
import queue
import socket

import aiohttp
import requests

from . import core
from . import mgmt
from .err import Error


class HSAPIClient:
    def __init__(self, server_name='hs.rerobots.net', server_port=443, verify_certs=True, event_loop=None):
        if event_loop is None:
            self.loop = asyncio.get_event_loop()
        else:
            self.loop = event_loop
        self.base_uri = 'https://{}:{}'.format(server_name, server_port)
        self.verify_certs = verify_certs
        self.local_config = mgmt.get_local_config()
        if len(self.local_config['keys']) > 0:
            self.default_key_index = 0
            with open(self.local_config['keys'][self.default_key_index]) as fp:
                self._cached_key = fp.read().strip()
        else:
            self.default_key_index = None
            self._cached_key = None

    def _add_key_header(self, headers=None):
        if headers is None:
            headers = dict()
        if self.default_key_index is not None:
            headers['Authorization'] = 'Bearer ' + self._cached_key
        return headers

    def get_remote_config(self):
        headers = self._add_key_header()
        res = requests.get(self.base_uri + '/list', headers=headers, verify=self.verify_certs)
        if res.ok:
            return res.json()
        else:
            raise Error('error contacting hardshare server: {}'.format(res.status_code))

    def sync_config(self):
        mgmt.modify_local(self.local_config)

    def register_new(self, at_most_one=True):
        """register new workspace deployment

        If at_most_one, then the local configuration can only declare
        one workspace deployment, and calls to this function will fail
        if there is already one.

        Examples where setting at_most_one=False might be the right
        choice: your local host is part of more than one robot, or it
        is part of more than one significantly distinct testbed.
        """
        if at_most_one and len(self.local_config['wdeployments']) > 0:
            raise Error('local configuration already declares a workspace deployment (and at_most_one=True)')
        headers = self._add_key_header()
        res = requests.post(self.base_uri + '/register', headers=headers, verify=self.verify_certs)
        if res.ok:
            payload = res.json()
            assert 'id' in payload
            self.local_config['wdeployments'].append({
                'id': payload['id'],
                'owner': payload['owner'],
            })
            self.sync_config()
        else:
            raise Error('error contacting hardshare server: {}'.format(res.status_code))
        return payload['id']

    def check_registration(self, id_prefix=None):
        if id_prefix is None:
            if len(self.local_config['wdeployments']) == 0:
                raise ValueError('no identifier given, and none in local config')
            else:
                id_prefix = self.local_config['wdeployments'][0]['id']
        headers = self._add_key_header()
        res = requests.get(self.base_uri + '/check/{}'.format(id_prefix), headers=headers, verify=self.verify_certs)
        if res.ok:
            pass
        elif res.status_code == 404:
            return {'err': 'not found', 'id_prefix': id_prefix}
        else:
            raise Error('error contacting hardshare server: {}'.format(res.status_code))
        return res.json()

    def terminate(self):
        base_path = '~/.rerobots'
        base_path = os.path.expanduser(base_path)
        to_addr = os.path.join(base_path, 'hardshare.sock')
        hss = socket.socket(family=socket.AF_UNIX, type=socket.SOCK_STREAM)
        hss.setblocking(False)
        try:
            hss.connect(to_addr)
            hss.send(b'TERMINATE\n')
        except BrokenPipeError:
            pass
        finally:
            hss.close()

    async def handle_dsocket(self, cq):
        base_path = '~/.rerobots'
        base_path = os.path.expanduser(base_path)
        hss = socket.socket(family=socket.AF_UNIX, type=socket.SOCK_STREAM)
        hss.setblocking(False)
        hss.bind(os.path.join(base_path, 'hardshare.sock'))
        try:
            hss.listen()
            while True:
                try:
                    conn, from_addr = await self.loop.sock_accept(hss)
                except:
                    continue
                conn.setblocking(False)
                try:
                    msg = await self.loop.sock_recv(conn, 1024)
                    msg = msg.decode()
                    if msg == 'STATUS\n':
                        await self.loop.sock_sendall(conn, b'OK\n')

                    elif msg == 'TERMINATE\n':
                        print('received request: TERMINATE')
                        await cq.put({'action': 'TERMINATE'})
                        conn.close()
                        break

                except BrokenPipeError:
                    pass
                finally:
                    conn.close()

        except:
            raise
        hss.close()

    def run_sync(self, id_prefix=None):
        try:
            self.loop.create_task(self.run(id_prefix=id_prefix))
            self.loop.run_forever()
        except KeyboardInterrupt:
            pass

    async def handle_wsrecv(self, ws, msg):
        if (msg.type == aiohttp.WSMsgType.CLOSED
            or msg.type == aiohttp.WSMsgType.ERROR):
            print('WebSocket CLOSED or ERROR')
            return

        try:
            payload = json.loads(msg.data)
            assert 'v' in payload and payload['v'] == 0
            assert 'cmd' in payload
        except:
            await ws.close()
            raise ValueError('ERROR: failed to parse message payload.')
            return

        if payload['cmd'] == 'INSTANCE_LAUNCH':
            if self.current is None:
                self.current = core.WorkspaceInstance()
                self.loop.create_task(self.current.launch_instance(
                    instance_id=payload['id'],
                    ws_send=ws.send_str,
                    conntype=payload['ct'],
                    publickey=payload['pr']
                ))
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'ACK',
                    'mi': payload['mi'],
                }))
            else:
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))

        elif payload['cmd'] == 'INSTANCE_DESTROY':
            if self.current is None:
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))
            else:
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'ACK',
                    'mi': payload['mi'],
                }))
                await self.current.destroy_instance()
                self.current = None

        elif payload['cmd'] == 'INSTANCE_STATUS':
            if self.current is None:
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))
            else:
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'ACK',
                    'mi': payload['mi'],
                    's': self.current.status,
                }))

        else:
            await ws.close()
            raise ValueError('ERROR: unknown command: {}'.format(payload['cmd']))


    async def run(self, id_prefix=None):
        cq = asyncio.Queue()
        self.loop.create_task(self.handle_dsocket(cq))
        self.current = None
        if id_prefix is None:
            if len(self.local_config['wdeployments']) == 0:
                raise ValueError('no identifier given, and none in local config')
            else:
                wdeployment_config = self.local_config['wdeployments'][0]
        else:
            wdeployment_config = None
            for local_wd_config in self.local_config['wdeployments']:
                if local_wd_config['id'].startswith(id_prefix):
                    wdeployment_config = local_wd_config
                    break
            if wdeployment_config is None:
                raise ValueError('workspace deployment {} not declared '
                                 'in local config'.format(id_prefix))
        headers = self._add_key_header()
        if self.verify_certs:
            session = aiohttp.ClientSession(headers=headers)
        else:
            conn = aiohttp.TCPConnector(verify_ssl=False)
            session = aiohttp.ClientSession(connector=conn, headers=headers)
        uri = self.base_uri + '/ad/{}'.format(wdeployment_config['id'])
        try:
            async with session.ws_connect(uri) as ws:
                futures = {
                    'ws.receive': self.loop.create_task(ws.receive()),
                    'cq.get': self.loop.create_task(cq.get()),
                }
                _exit = False
                while not _exit:
                    done, pending = await asyncio.wait(futures.values(), loop=self.loop, return_when=FIRST_COMPLETED)
                    for done_future in done:
                        if done_future == futures['ws.receive']:
                            futures['ws.receive'] = self.loop.create_task(ws.receive())
                            await self.handle_wsrecv(ws, done_future.result())

                        else:  # done_future == futures['cq.get']
                            futures['cq.get'] = self.loop.create_task(cq.get())
                            msg = done_future.result()
                            if msg['action'] == 'TERMINATE':
                                _exit = True
                                break

                for future in futures.values():
                    future.cancel()

        finally:
            await session.close()
        self.loop.stop()
