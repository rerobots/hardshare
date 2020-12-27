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
import hashlib
import json
import logging
import os
import os.path
import signal
import socket
import time

import aiohttp

from . import core
from . import mgmt
from .err import Error


logger = logging.getLogger(__name__)


class HSAPIClient:
    def __init__(self, event_loop=None):
        if event_loop is None:
            self.loop = asyncio.get_event_loop()
        else:
            self.loop = event_loop
        self.base_uri = 'https://hs.rerobots.net'
        self.session = None
        logger.debug('instantiating client object for hardshare server at {}'
                     .format(self.base_uri))
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
        self.loop.run_until_complete(self.async_init())


    async def async_init(self):
        self.session = aiohttp.ClientSession(headers=self._add_key_header())


    def __del__(self):
        if self.session:
            self.loop.run_until_complete(self.session.close())


    def _add_key_header(self, headers=None):
        if headers is None:
            headers = dict()
        if self.default_key_index is not None:
            headers['Authorization'] = 'Bearer ' + self._cached_key
        return headers

    def get_remote_config(self, include_dissolved=True):
        if include_dissolved:
            res = self.loop.run_until_complete(self.session.get(self.base_uri + '/list?with_dissolved'))
        else:
            res = self.loop.run_until_complete(self.session.get(self.base_uri + '/list'))
        if res.status == 200:
            payload = self.loop.run_until_complete(res.json())
        elif res.status == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            return {'err': err}
        else:
            raise Error('error contacting hardshare server: {}'.format(res.status))
        if include_dissolved:
            res = self.loop.run_until_complete(self.session.get('https://api.rerobots.net/hardshare/list?with_dissolved'))
        else:
            res = self.loop.run_until_complete(self.session.get('https://api.rerobots.net/hardshare/list'))
        if res.status == 200:
            hlist = self.loop.run_until_complete(res.json())
            for jj, wd in enumerate(payload['deployments']):
                payload['deployments'][jj]['desc'] = hlist['attr'][wd['id']]['desc']
            return payload
        elif res.status == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            return {'err': err}
        else:
            raise Error('error contacting core API server: {}'.format(res.status))

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
        res = self.loop.run_until_complete(self.session.post(self.base_uri + '/register'))
        if res.status == 200:
            payload = self.loop.run_until_complete(res.json())
            assert 'id' in payload
            self.local_config['wdeployments'].append({
                'id': payload['id'],
                'owner': payload['owner'],
            })
            self.sync_config()
        elif res.status == 400:
            try:
                err = self.loop.run_until_complete(res.json())['error_message']
            except:
                raise Error('error contacting hardshare server: {}'.format(res.status))
            raise Error(err)
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status))
        return payload['id']

    def declare_existing(self, id_prefix):
        res = self.loop.run_until_complete(self.session.get(self.base_uri + '/list'))
        if res.status == 200:
            payload = self.loop.run_until_complete(res.json())
        elif res.status_code == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            return {'err': err}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status))
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
        res = self.loop.run_until_complete(self.session.get(self.base_uri + '/check/{}'.format(id_prefix)))
        if res.status == 200:
            pass
        elif res.status == 404:
            return {'err': 'not found', 'id_prefix': id_prefix}
        elif res.status == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            return {'err': err, 'id_prefix': id_prefix}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status))
        return self.loop.run_until_complete(res.json())

    def dissolve_registration(self, wdid=None):
        if wdid is None:
            if len(self.local_config['wdeployments']) == 0:
                msg = 'no identifier given, and none in local config'
                logger.error('HSAPIClient.check_registration(): {}'.format(msg))
                raise ValueError(msg)
            else:
                wdid = self.local_config['wdeployments'][0]['id']
        res = self.loop.run_until_complete(self.session.post(self.base_uri + '/dis/{}'.format(wdid)))
        if res.status == 200:
            pass
        elif res.status == 404:
            return {'err': 'not found', 'wdid': wdid}
        elif res.status == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            return {'err': err, 'wdid': wdid}
        else:
            raise Error('error contacting hardshare server: {}'
                        .format(res.status))
        return self.loop.run_until_complete(res.json())

    def get_access_rules(self, wdid=None):
        if wdid is None:
            if len(self.local_config['wdeployments']) == 0:
                msg = 'no identifier given, and none in local config'
                logger.error('HSAPIClient.check_registration(): {}'.format(msg))
                raise ValueError(msg)
            else:
                wdid = self.local_config['wdeployments'][0]['id']
        res = self.loop.run_until_complete(self.session.get('https://api.rerobots.net/deployment/{}/rules'.format(wdid)))
        if res.status == 200:
            pass
        elif res.status == 404:
            return {'err': 'not found', 'wdid': wdid}
        elif res.status == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            return {'err': err, 'wdid': wdid}
        else:
            raise Error('error contacting rerobots server: {}'
                        .format(res.status))
        return self.loop.run_until_complete(res.json())

    def add_access_rule(self, wdid=None, to_user=None):
        if wdid is None:
            if len(self.local_config['wdeployments']) == 0:
                msg = 'no identifier given, and none in local config'
                logger.error('HSAPIClient.check_registration(): {}'.format(msg))
                raise ValueError(msg)
            else:
                wdid = self.local_config['wdeployments'][0]['id']
                if to_user is None:
                    to_user = self.local_config['wdeployments'][0]['owner']
        elif to_user is None:
            for wd in self.local_config['wdeployments']:
                if wd['id'] == wdid:
                    to_user = wd['owner']
                    break
            if to_user is None:
                raise ValueError('to_user None but given wdeployment not in local config')
        body = {'cap': 'CAP_INSTANTIATE'}
        if to_user:
            body['user'] = to_user
        res = self.loop.run_until_complete(self.session.post(
            'https://api.rerobots.net/deployment/{}/rule'.format(wdid),
            json=body,
        ))
        if res.status == 200:
            return
        elif res.status == 404:
            raise Error('not found')
        elif res.status == 400:
            err = self.loop.run_until_complete(res.json())['error_message']
            raise Error('error contacting rerobots server: {}'.format(err))
        else:
            raise Error('error contacting rerobots server: {}'
                        .format(res.status))

    def drop_access_rules(self, wdid=None):
        if wdid is None:
            if len(self.local_config['wdeployments']) == 0:
                msg = 'no identifier given, and none in local config'
                logger.error('HSAPIClient.check_registration(): {}'.format(msg))
                raise ValueError(msg)
            else:
                wdid = self.local_config['wdeployments'][0]['id']
        res = self.loop.run_until_complete(self.session.get('https://api.rerobots.net/deployment/{}/rules'.format(wdid)))
        if res.status != 200:
            raise Error('error contacting rerobots server: ', res.status)
        payload = self.loop.run_until_complete(res.json())
        for rule in payload['rules']:
            res = self.loop.run_until_complete(self.session.delete(
                'https://api.rerobots.net/deployment/{}/rule/{}'.format(wdid, rule['id'])
            ))
            if res.status != 200:
                raise Error('error deleting rule {}: '.format(rule['id'], res.status))

    def terminate(self, wdid=None):
        if wdid is None:
            sockname = 'hardshare.sock'
        else:
            sockname = 'hardshare.{}.sock'.format(wdid)
        to_addr = os.path.join(os.path.expanduser('~'), '.rerobots', sockname)
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
        assert self.current_wdeployment is not None
        sockname = 'hardshare.{}.sock'.format(self.current_wdeployment['id'])
        socket_path = os.path.join(os.path.expanduser('~'), '.rerobots', sockname)
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
                        if self.current is None:
                            await self.loop.sock_sendall(conn, b'READY\n')
                        else:
                            await self.loop.sock_sendall(conn, bytes('ACTIVE:{}\n'.format(self.current.container_name), encoding='utf-8'))

                    elif msg == 'TERMINATE\n':
                        logger.info('received request: TERMINATE')
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
        self.main = self.loop.create_task(self.run())
        self.dsocket = self.loop.create_task(self.handle_dsocket(self.main))
        logger.debug('started async run()')
        signal.signal(signal.SIGTERM, self.handle_sigterm)
        try:
            self.loop.run_until_complete(self.main)
        except KeyboardInterrupt:
            self.main.cancel()
            self.loop.run_until_complete(self.main)
        except ValueError as e:
            logger.error('caught ValueError: {}'.format(e))
            return 1
        finally:
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
                if payload['ct'] == 'sshtun':
                    if 'ssh_key' not in self.local_config or self.local_config['ssh_key'] is None:
                        logger.warning('request to launch with connection type sshtun, '
                                       'but local configuration declares no SSH key')
                        logger.debug('in response to INSTANCE_LAUNCH, sending NACK')
                        await ws.send_str(json.dumps({
                            'v': 0,
                            'cmd': 'NACK',
                            'mi': payload['mi'],
                        }))
                        return True
                    if not os.path.exists(self.local_config['ssh_key']):
                        logger.warning('key path for sshtun appears to not exist: {}'.format(self.local_config['ssh_key']))

                self.ws_recvmap[payload['id']] = asyncio.Queue()
                cprovider = self.current_wdeployment['cprovider']
                cargs = self.current_wdeployment.get('cargs', None)
                image = self.current_wdeployment.get('image', None)
                terminate = self.current_wdeployment.get('terminate', None)
                init_inside = self.current_wdeployment.get('init_inside', [])
                self.current = core.WorkspaceInstance(cprovider=cprovider, cargs=cargs, image=image, terminate=terminate)
                self.loop.create_task(self.current.launch_instance(
                    instance_id=payload['id'],
                    ws_send=ws.send_str,
                    ws_recv=self.ws_recvmap[payload['id']],
                    conntype=payload['ct'],
                    initial_publickey=payload['pr'],
                    init_inside=init_inside,
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


    async def run(self):
        logger.info('start to advertise wdeployment {}'.format(self.current_wdeployment['id']))
        headers = self._add_key_header()
        uri = self.base_uri + '/ad/{}'.format(self.current_wdeployment['id'])
        active = True
        lost_connection = None
        while active:
            session = aiohttp.ClientSession(headers=headers)
            try:
                async with session.ws_connect(uri, receive_timeout=45, autoping=True) as ws:
                    if lost_connection is not None:
                        lost_connection  = None
                    async for msg in ws:
                        if not (await self.handle_wsrecv(ws, msg)):
                            break

            except asyncio.CancelledError:
                active = False

            except Exception as e:
                logger.error('caught {}: {}'.format(type(e), e))
                if lost_connection is None:
                    lost_connection = time.time()
                else:
                    if time.time() - lost_connection > 1200:  # 20 minutes
                        logger.error('giving up re-connection attempts')
                        break
                await asyncio.sleep(1)

            finally:
                await session.close()
