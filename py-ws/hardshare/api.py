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
import hashlib
import json
import logging
import os
import signal
import socket

import aiohttp
import requests

from . import core
from . import mgmt
from .err import Error


logger = logging.getLogger(__name__)


class HSAPIClient:
    def __init__(self, server_name='hs.rerobots.net', server_port=443, verify_certs=True, event_loop=None):
        if event_loop is None:
            self.loop = asyncio.get_event_loop()
        else:
            self.loop = event_loop
        self.base_uri = 'https://{}:{}'.format(server_name, server_port)
        logger.debug('instantiating client object for hardshare server at {}'
                     .format(self.base_uri))
        self.verify_certs = verify_certs
        self.ws_recvmap = dict()
        self.local_config = mgmt.get_local_config()
        if len(self.local_config['keys']) > 0:
            self.default_key_index = 0
            with open(self.local_config['keys'][self.default_key_index]) as fp:
                self._cached_key = fp.read().strip()
            if logger.isEnabledFor(logging.DEBUG):
                md5 = hashlib.md5(self._cached_key.encode()).hexdigest()
                logger.debug('local API keys available. using the first one,'
                             ' with MD5 hash {}'.format(md5))
        else:
            self.default_key_index = None
            self._cached_key = None
        self.current_wdeployment = None

    def _add_key_header(self, headers=None):
        if headers is None:
            headers = dict()
        if self.default_key_index is not None:
            headers['Authorization'] = 'Bearer ' + self._cached_key
        return headers

    def get_remote_config(self):
        headers = self._add_key_header()
        res = requests.get(self.base_uri + '/list',
                           headers=headers, verify=self.verify_certs)
        if res.ok:
            return res.json()
        elif res.status_code == 400:
            return {'err': res.json()['error_message']}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status_code))

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
            raise Error('local configuration already declares a'
                        ' workspace deployment (and at_most_one=True)')
        headers = self._add_key_header()
        res = requests.post(self.base_uri + '/register',
                            headers=headers, verify=self.verify_certs)
        if res.ok:
            payload = res.json()
            assert 'id' in payload
            self.local_config['wdeployments'].append({
                'id': payload['id'],
                'owner': payload['owner'],
            })
            self.sync_config()
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status_code))
        return payload['id']

    def declare_existing(self, id_prefix):
        headers = self._add_key_header()
        res = requests.get(self.base_uri + '/list',
                           headers=headers, verify=self.verify_certs)
        if res.ok:
            payload = res.json()
        elif res.status_code == 400:
            return {'err': res.json()['error_message']}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status_code))
        for wd in payload['deployments']:
            if wd['id'].startswith(id_prefix):
                id_prefix = wd['id']
                for wd_local in self.local_config['wdeployments']:
                    if id_prefix == wd_local['id']:
                        logger.warning('attempted to declare workspace deployment that is already declared in local configuration')
                        return
                self.local_config['wdeployments'].append({
                    'owner': payload['owner'],
                    'id': id_prefix,
                })
                return
        msg = 'no previously registered workspace deployments found with id prefix {}'.format(id_prefix)
        logger.error(msg)
        raise Error(msg)

    def check_registration(self, id_prefix=None):
        if id_prefix is None:
            if len(self.local_config['wdeployments']) == 0:
                msg = 'no identifier given, and none in local config'
                logger.error('HSAPIClient.check_registration(): {}'.format(msg))
                raise ValueError(msg)
            else:
                id_prefix = self.local_config['wdeployments'][0]['id']
        headers = self._add_key_header()
        res = requests.get(self.base_uri + '/check/{}'.format(id_prefix),
                           headers=headers, verify=self.verify_certs)
        if res.ok:
            pass
        elif res.status_code == 404:
            return {'err': 'not found', 'id_prefix': id_prefix}
        elif res.status_code == 400:
            return {'err': res.json()['error_message'], 'id_prefix': id_prefix}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status_code))
        return res.json()

    def dissolve_registration(self, id_prefix=None):
        if id_prefix is None:
            if len(self.local_config['wdeployments']) == 0:
                msg = 'no identifier given, and none in local config'
                logger.error('HSAPIClient.check_registration(): {}'.format(msg))
                raise ValueError(msg)
            else:
                id_prefix = self.local_config['wdeployments'][0]['id']
        headers = self._add_key_header()
        res = requests.post(self.base_uri + '/dis/{}'.format(id_prefix),
                            headers=headers, verify=self.verify_certs)
        if res.ok:
            pass
        elif res.status_code == 404:
            return {'err': 'not found', 'id_prefix': id_prefix}
        elif res.status_code == 400:
            return {'err': res.json()['error_message'], 'id_prefix': id_prefix}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status_code))
        return res.json()

    def terminate(self):
        to_addr = os.path.join(os.path.expanduser('~'), '.rerobots', 'hardshare.sock')
        hss = socket.socket(family=socket.AF_UNIX, type=socket.SOCK_STREAM)
        hss.setblocking(False)
        try:
            hss.connect(to_addr)
            hss.send(b'TERMINATE\n')
        except BrokenPipeError:
            pass
        finally:
            hss.close()

    async def handle_dsocket(self, main):
        socket_path = os.path.join(os.path.expanduser('~'), '.rerobots', 'hardshare.sock')
        hss = socket.socket(family=socket.AF_UNIX, type=socket.SOCK_STREAM)
        hss.setblocking(False)
        hss.bind(socket_path)
        try:
            hss.listen()
            while True:
                conn, from_addr = await self.loop.sock_accept(hss)
                conn.setblocking(False)
                try:
                    msg = await self.loop.sock_recv(conn, 1024)
                    msg = msg.decode()
                    if msg == 'STATUS\n':
                        await self.loop.sock_sendall(conn, b'OK\n')

                    elif msg == 'TERMINATE\n':
                        print('received request: TERMINATE')
                        main.cancel()
                        conn.close()
                        break

                except BrokenPipeError:
                    pass
                finally:
                    conn.close()

        except asyncio.CancelledError:
            pass

        finally:
            hss.close()
            os.unlink(socket_path)


    def handle_sigterm(self, signum, frame):
        self.main.cancel()


    def run_sync(self, id_prefix=None):
        logger.debug('entered run_sync()')
        self.main = self.loop.create_task(self.run(id_prefix=id_prefix))
        self.dsocket = self.loop.create_task(self.handle_dsocket(self.main))
        logger.debug('started async run()')
        signal.signal(signal.SIGTERM, self.handle_sigterm)
        try:
            self.loop.run_until_complete(self.main)
        except KeyboardInterrupt:
            self.main.cancel()
            self.loop.run_until_complete(self.main)
        except ValueError as e:
            print(e)
            return 1
        self.dsocket.cancel()
        self.loop.run_until_complete(self.dsocket)
        return 0


    async def handle_wsrecv(self, ws, msg):
        if msg.type == aiohttp.WSMsgType.CLOSED:
            logger.info('WebSocket CLOSED')
            return False
        elif msg.type == aiohttp.WSMsgType.ERROR:
            logger.error('WebSocket ERROR')
            return False

        logger.debug('WebSocket: received {}'.format(msg.data))
        try:
            payload = json.loads(msg.data)
            assert 'v' in payload and payload['v'] == 0
            assert 'cmd' in payload
        except:
            await ws.close()
            raise ValueError('ERROR: failed to parse message payload.')

        if payload['cmd'] == 'INSTANCE_LAUNCH':
            if self.current is None:
                self.ws_recvmap[payload['id']] = asyncio.Queue()
                self.current = core.WorkspaceInstance(cprovider=self.current_wdeployment['cprovider'])
                self.loop.create_task(self.current.launch_instance(
                    instance_id=payload['id'],
                    ws_send=ws.send_str,
                    ws_recv=self.ws_recvmap[payload['id']],
                    conntype=payload['ct'],
                    initial_publickey=payload['pr'],
                    tunnelkey_path=self.local_config.get('ssh_key', None)
                ))
                logger.debug('in response to INSTANCE_LAUNCH, sending ACK')
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'ACK',
                    'mi': payload['mi'],
                }))
            else:
                logger.debug('in response to INSTANCE_LAUNCH, sending NACK')
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
                logger.debug('in response to INSTANCE_STATUS, sending NACK')
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))
            else:
                logger.debug('in response to INSTANCE_LAUNCH,'
                             ' sending status: {}'.format(self.current.status))
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'ACK',
                    'mi': payload['mi'],
                    's': self.current.status,
                }))

        elif payload['cmd'] == 'TH_ACCEPT':
            if (self.current is None or 'id' not in payload
                or self.current.instance_id != payload['id']):
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))
            else:
                await self.ws_recvmap[self.current.instance_id].put(payload)

        elif payload['cmd'] == 'TH_PING':
            resp = {
                'v': 0,
                'thid': payload['thid'],
                'id': self.current_wdeployment['id'],
                'mi': payload['mi'],
            }
            if (self.current is None or 'id' not in payload
                or self.current_wdeployment['id'] != payload['id']
                or self.current.tunnelhub is None
                or self.current.tunnelhub['id'] != payload['thid']):
                resp['cmd'] = 'NACK'
            else:
                resp['cmd'] = 'ACK'
            await ws.send_str(json.dumps(resp))
            logger.debug('WebSocket: sent response: {}'.format(resp))

        elif payload['cmd'] == 'VPN_CREATE':
            if (self.current is None or 'id' not in payload
                or self.current.instance_id != payload['id']):
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))
            else:
                await self.ws_recvmap[self.current.instance_id].put(payload)

        elif payload['cmd'] == 'VPN_NEWCLIENT':
            if (self.current is None or 'id' not in payload
                or self.current.instance_id != payload['id']):
                await ws.send_str(json.dumps({
                    'v': 0,
                    'cmd': 'NACK',
                    'mi': payload['mi'],
                }))
            else:
                await self.ws_recvmap[self.current.instance_id].put(payload)

        else:
            await ws.close()
            raise ValueError('ERROR: unknown command: {}'.format(payload['cmd']))

        return True


    async def run(self, id_prefix=None):
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
        self.current_wdeployment = wdeployment_config
        headers = self._add_key_header()
        if self.verify_certs:
            session = aiohttp.ClientSession(headers=headers)
        else:
            conn = aiohttp.TCPConnector(verify_ssl=False)
            session = aiohttp.ClientSession(connector=conn, headers=headers)
        uri = self.base_uri + '/ad/{}'.format(wdeployment_config['id'])
        try:
            async with session.ws_connect(uri, timeout=90.0, autoping=True) as ws:
                async for msg in ws:
                    if not (await self.handle_wsrecv(ws, msg)):
                        break

        except asyncio.CancelledError:
            pass

        finally:
            await session.close()
