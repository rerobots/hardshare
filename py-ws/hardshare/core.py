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
"""instance lifecycle
"""
import asyncio
import json
import logging
import os
import os.path
import socket
import subprocess
import tempfile
import time
import uuid


logger = logging.getLogger(__name__)


class WorkspaceInstance:
    def __init__(self, cprovider=None, event_loop=None):
        if event_loop is None:
            self.loop = asyncio.get_event_loop()
        else:
            self.loop = event_loop
        self.cprovider = cprovider
        self.status = 'INIT'
        self.container_name = 'rrc'
        self.instance_id = None
        self.tunnelhub = None
        self.tunnel_task = None

    @classmethod
    def inspect_instance(cls, wdeployment=None):
        """detect whether local host is running a workspace instance

        Return dict that describes findings.
        """
        if wdeployment is None:
            cprovider = 'docker'  # TODO: reasonable default?
        else:
            cprovider = wdeployment['cprovider']
        findings = {
            'daemon_found': False,
            'provider': cprovider,
        }
        base_path = os.path.join(os.path.expanduser('~'), '.rerobots')
        to_addr = os.path.join(base_path, 'hardshare.sock')
        hss = socket.socket(family=socket.AF_UNIX, type=socket.SOCK_STREAM)
        hss.settimeout(10)
        try:
            hss.connect(to_addr)
            hss.send(b'STATUS\n')
            msg = hss.recv(1024)
            findings['daemon_found'] = True
        except socket.timeout:
            pass
        except BrokenPipeError:
            pass
        except FileNotFoundError:
            pass
        finally:
            hss.close()
        empty_default = cls()
        if cprovider in ['docker', 'podman']:
            try:
                cp = subprocess.run([cprovider, 'inspect', empty_default.container_name],
                                    stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT,
                                    universal_newlines=True)
                if cp.returncode == 0:
                    findings['has_instance'] = True
                    cinfo = json.loads(cp.stdout)[0]
                    findings['container'] = {
                        'name': empty_default.container_name,
                        'id': cinfo['Id'],
                        'created': cinfo['Created'],
                        'image_id': cinfo['Image'],
                    }
                    cp = subprocess.run([cprovider, 'image', 'inspect', cinfo['Image']],
                                        stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT,
                                        universal_newlines=True)
                    if cp.returncode == 0:
                        iminfo = json.loads(cp.stdout)[0]
                        findings['container']['image_tags'] = iminfo['RepoTags']
                else:
                    findings['has_instance'] = False
            except FileNotFoundError:
                findings['has_instance'] = False
                findings['warnings'] = 'cprovider {} not found. Is it installed?'.format(cprovider)
        else:
            findings['has_instance'] = False
            findings['warnings'] = 'cprovider "{}" not known'.format(cprovider)
        return findings


    async def get_container_addr(self, timeout=60):
        logger.info('attempting to get IPv4 address of container...'
                    ' (entered get_container_addr())')
        if self.cprovider not in ['docker', 'podman']:
            raise ValueError('unknown cprovider: {}'.format(self.cprovider))
        container_inspect = await asyncio.create_subprocess_exec(
            self.cprovider, 'inspect', self.container_name,
            stdout=subprocess.PIPE
        )
        stdout_data, stderr_data = await container_inspect.communicate()
        cdata = json.loads(str(stdout_data, encoding='utf-8'))
        if len(cdata) < 1 or 'NetworkSettings' not in cdata[0] or 'IPAddress' not in cdata[0]['NetworkSettings']:
            logger.info('did not find IPv4 or IPv6 address before timeout of {} s'.format(timeout))
            return None
        else:
            logger.info('found address: {}'.format(cdata[0]['NetworkSettings']['IPAddress']))
            return cdata[0]['NetworkSettings']['IPAddress']


    async def get_container_sshport(self, timeout=60):
        logger.info('attempting to get SSH port of container...')
        if self.cprovider not in ['docker', 'podman']:
            raise ValueError('unknown cprovider: {}'.format(self.cprovider))
        start_time = time.time()
        while time.time() - start_time < timeout:
            container_inspect = await asyncio.create_subprocess_exec(
                self.cprovider, 'port', self.container_name, '22',
                stdout=subprocess.PIPE
            )
            stdout_data, stderr_data = await container_inspect.communicate()
            if container_inspect.returncode == 0:
                fwdaddrport = str(stdout_data, encoding='utf-8')
                fwdaddr, fwdport = fwdaddrport.split(':')
                return int(fwdport)
            await asyncio.sleep(1)
        logger.info('did not find SSH port before timeout of {} s'.format(timeout))
        return None


    async def get_container_hostkey(self, timeout=120):
        logger.info('attempting to get hostkey from container...'
                    ' (entered get_container_hostkey())')
        if self.cprovider not in ['docker', 'podman']:
            raise ValueError('unknown cprovider: {}'.format(self.cprovider))
        hostkey_filename = 'ssh_host_ecdsa_key.pub'
        gethostkey_command = [self.cprovider, 'cp', self.container_name + ':/etc/ssh/' + hostkey_filename, '.']
        start_time = time.time()
        while time.time() - start_time < timeout:
            c_gethostkey = await asyncio.create_subprocess_exec(
                *gethostkey_command,
                stdout=subprocess.PIPE
            )
            rt = await c_gethostkey.wait()
            if rt != 0:
                await asyncio.sleep(1)
                continue
            logger.info('found hostkey!')
            with open(hostkey_filename, 'rt', encoding='utf-8') as fp:
                return fp.read().strip()
        logger.info('did not find hostkey before timeout of {} s'.format(timeout))
        return None


    async def find_tunnelhub(self, ws_send, ws_recv):
        assert self.tunnelhub is None
        logger.debug('sending TH_SEARCH')
        payload = {
            'v': 0,
            'cmd': 'TH_SEARCH',
            'id': self.instance_id,
            'mo': self.conntype,
        }
        if self.tunnelkey_public:
            payload['key'] = self.tunnelkey_public
        await ws_send(json.dumps(payload))
        res = await ws_recv.get()
        assert res['v'] == 0
        assert res['id'] == self.instance_id
        assert res['cmd'] == 'TH_ACCEPT'
        logger.debug('received TH_ACCEPT for th {} at {}'.format(
            res['thid'],
            res['ipv4']
        ))
        self.tunnelhub = {
            'id': res['thid'],
            'ipv4': res['ipv4'],
            'hostkey': res['hostkey'],
            'listen_port': res['port'],
            'connect_port': res['thport'],
            'connect_user': res['thuser'],
        }
        await ws_send(json.dumps({
            'v': 0,
            'cmd': 'ACK',
            'mi': res['mi'],
        }))


    async def maintain_tunnel(self, ws_send, ws_recv):
        try:
            while self.container_addr is None:
                await asyncio.sleep(1)
        except asyncio.CancelledError:
            return

        sshtunnel = None

        if self.tunnelhub is None:
            logger.info('attempting to associate with a tunnel hub')
            await self.find_tunnelhub(ws_send, ws_recv)
            assert self.tunnelhub is not None
            logger.info('associated with tunnel hub {}'.format(self.tunnelhub['id']))

        tunnel_command = ('ssh -o ServerAliveInterval=10 '
                          '-o StrictHostKeyChecking=no '
                          '-o ExitOnForwardFailure=yes '
                          '-T -N '
                          '-R :{THPORT}:{CONTAINERADDR}:{CONTAINERPORT} '
                          '-i {TUNNELKEY_PATH} '
                          '-p {TH_INFRA_PORT} '
                          '{THUSER}@{THADDR}'.format(
                              THADDR=self.tunnelhub['ipv4'],
                              THPORT=self.tunnelhub['listen_port'],
                              TUNNELKEY_PATH=self.tunnelkey_path,
                              TH_INFRA_PORT=self.tunnelhub['connect_port'],
                              CONTAINERADDR=self.container_addr,
                              CONTAINERPORT=self.container_port_ssh,
                              THUSER=self.tunnelhub['connect_user'])).split()

        logger.info('opening ssh tunnel from port {THPORT} '
                    'of {THADDR} to container '
                    'at {CONTAINER}'.format(THADDR=self.tunnelhub['ipv4'],
                                            THPORT=self.tunnelhub['listen_port'],
                                            CONTAINER=self.container_addr))

        sshtunnel = await asyncio.create_subprocess_exec(*tunnel_command)
        self.status = 'READY'
        logger.info('marked instance as {}'.format(self.status))
        await ws_send(json.dumps({
            'v': 0,
            'cmd': 'INSTANCE_STATUS',
            's': self.status
        }))

        try:
            while True:
                if sshtunnel.returncode is None:
                    await asyncio.sleep(5)
                else:
                    logger.warning('tunnel process unexpectedly exited'
                                   ' with returncode {}'
                                   .format(sshtunnel.returncode))
                    sshtunnel = await asyncio.create_subprocess_exec(*tunnel_command)

        except asyncio.CancelledError:
            if sshtunnel is not None:
                if sshtunnel.returncode is None:
                    sshtunnel.terminate()
                    await sshtunnel.wait()
                if sshtunnel.returncode != 0:
                    logger.warning('tunnel subprocess failed '
                                   'with nonzero exit code: {}'
                                   .format(sshtunnel.returncode))
            await ws_send(json.dumps({
                'v': 0,
                'cmd': 'SSHTUN_DELETE',
            }))


    async def start_vpn(self, ws_send, ws_recv):
        if self.cprovider not in ['docker', 'podman']:
            raise ValueError('unknown cprovider: {}'.format(self.cprovider))
        try:
            while self.container_addr is None:
                await asyncio.sleep(1)
        except asyncio.CancelledError:
            return

        if self.tunnelhub is None:
            logger.info('attempting to associate with a tunnel hub')
            await self.find_tunnelhub(ws_send, ws_recv)
            assert self.tunnelhub is not None
            logger.info('associated with tunnel hub {}'.format(self.tunnelhub['id']))

        mi = str(uuid.uuid4())
        logger.debug('sending VPN_CREATE (mi: {})'.format(mi))
        await ws_send(json.dumps({
            'v': 0,
            'cmd': 'VPN_CREATE',
            'id': self.instance_id,
            'mi': mi,
        }))
        res = await ws_recv.get()
        assert res['v'] == 0
        assert res['mi'] == mi
        assert res['cmd'] == 'ACK'
        assert res['id'] == self.instance_id

        mi = str(uuid.uuid4())
        logger.debug('sending VPN_NEWCLIENT (mi: {})'.format(mi))
        await ws_send(json.dumps({
            'v': 0,
            'cmd': 'VPN_NEWCLIENT',
            'id': self.instance_id,
            'mi': mi,
        }))
        res = await ws_recv.get()
        assert res['v'] == 0
        assert res['mi'] == mi
        assert res['cmd'] == 'ACK'
        assert res['id'] == self.instance_id
        ovpn_config = res['ovpn']

        try:
            # Copy OVPN file into container
            fd, fname = tempfile.mkstemp()
            fp = os.fdopen(fd, 'wt')
            fp.write(ovpn_config)
            fp.close()
            subprocess.check_call([self.cprovider, 'cp',
                                   fname,
                                   self.container_name + ':/etc/' + self.container_name + '_client.ovpn'])
            os.unlink(fname)

            # Start client
            # ASSUME images for Docker provider already have
            # openvpn and avahi-daemon installed.
            pre_commands = [
                [self.cprovider, 'exec', self.container_name, '/etc/init.d/dbus', 'start'],
                [self.cprovider, 'exec', '-d', self.container_name, 'avahi-daemon']
            ]
            vpnclient_command = (self.cprovider + ' exec '
                                 + self.container_name
                                 + ' openvpn '
                                 '/etc/' + self.container_name + '_client.ovpn')

            for pre_cmd in pre_commands:
                subprocess.check_call(pre_cmd)

            vpnclient = await asyncio.create_subprocess_exec(*(vpnclient_command.split()))

            self.status = 'READY'
            logger.info('marked instance as {}'.format(self.status))
            await ws_send(json.dumps({
                'v': 0,
                'cmd': 'INSTANCE_STATUS',
                's': self.status
            }))

        except:
            self.status = 'INIT_FAIL'
            logger.info('marked instance as {}'.format(self.status))
            await ws_send(json.dumps({
                'v': 0,
                'cmd': 'INSTANCE_STATUS',
                's': self.status
            }))
            return

        try:
            while True:
                if vpnclient.returncode is None:
                    await asyncio.sleep(5)
                else:
                    vpnclient = await asyncio.create_subprocess_exec(*(vpnclient_command.split()))

        except asyncio.CancelledError:
            logger.debug('sending VPN_DELETE')
            await ws_send(json.dumps({
                'v': 0,
                'cmd': 'VPN_DELETE',
                'id': self.instance_id,
            }))


    async def launch_instance(self, instance_id, ws_send, ws_recv, conntype, initial_publickey, tunnelkey_path=None):
        if self.cprovider not in ['docker', 'podman']:
            raise ValueError('unknown cprovider: {}'.format(self.cprovider))

        self.conntype = conntype
        self.instance_id = instance_id

        try:
            self.tunnelkey_path = tunnelkey_path
            if self.tunnelkey_path:
                with open(self.tunnelkey_path + '.pub', 'r') as fp:
                    self.tunnelkey_public = fp.read()
            else:
                self.tunnelkey_public = None

            fd, fname = tempfile.mkstemp()
            fp = os.fdopen(fd, 'wt')
            fp.write(initial_publickey)
            fp.close()

            launch_args = [self.cprovider, 'run', '-d',
                           '-h', self.container_name,
                           '--name', self.container_name,
                           '--device=/dev/net/tun:/dev/net/tun',
                           '--cap-add=NET_ADMIN']
            if self.cprovider == 'podman':
                launch_args.extend(['-p', '127.0.0.1::22'])
            launch_args += ['hs.rerobots.net/generic:latest']
            logger.debug('subprocess: {}'.format(launch_args))
            subprocess.check_call(launch_args,
                                  stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL)

            if self.cprovider == 'docker':
                self.container_addr = await self.get_container_addr(timeout=10)
                self.container_port_ssh = 22
            elif self.cprovider == 'podman':
                self.container_addr = '127.0.0.1'
                self.container_port_ssh = await self.get_container_sshport(timeout=10)

            self.hostkey = await self.get_container_hostkey(timeout=45)
            assert self.container_addr is not None

            cexec = [self.cprovider, 'exec', self.container_name]
            prepare_commands = [cexec + ['/bin/bash', '-c', 'rm /etc/ssh/ssh_host_*'],
                                cexec + ['/usr/bin/ssh-keygen', '-A']]

            movekey_commands = [cexec + ['/bin/mkdir', '-p', '/root/.ssh'],
                                [self.cprovider, 'cp', fname, self.container_name + ':/root/.ssh/authorized_keys'],
                                cexec + ['/bin/chown', '0:0', '/root/.ssh/authorized_keys']]
            for command in prepare_commands + movekey_commands:
                logger.debug('subprocess: {}'.format(command))
                subprocess.check_call(command,
                                      stdout=subprocess.DEVNULL,
                                      stderr=subprocess.DEVNULL)

            os.unlink(fname)

        except Exception as e:
            logger.error('caught exception {}: {}'.format(type(e), e))
            self.status = 'INIT_FAIL'

        await ws_send(json.dumps({
            'v': 0,
            'cmd': 'INSTANCE_STATUS',
            's': self.status
        }))
        if self.conntype == 'vpn':
            self.tunnel_task = self.loop.create_task(self.start_vpn(ws_send, ws_recv))
        else:  # self.conntype == 'sshtun'
            self.tunnel_task = self.loop.create_task(self.maintain_tunnel(ws_send, ws_recv))


    async def destroy_instance(self):
        destroy_args = [self.cprovider, 'rm', '-f', self.container_name]
        subprocess.check_call(destroy_args,
                              stdout=subprocess.DEVNULL,
                              stderr=subprocess.DEVNULL)
        if self.tunnel_task is not None:
            self.tunnel_task.cancel()
            while not self.tunnel_task.done():
                await asyncio.sleep(0.5)
            self.tunnel_task = None
