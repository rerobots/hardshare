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
import os
import socket
import subprocess


class WorkspaceInstance:
    def __init__(self):
        self.status = 'INIT'
        self.container_name = 'rrc'
        self.instance_id = None

    @classmethod
    def inspect_instance(cls):
        """detect whether local host is running a workspace instance

        Return dict that describes findings.
        """
        findings = {
            'daemon_found': False,
            'provider': 'docker',
        }
        base_path = '~/.rerobots'
        base_path = os.path.expanduser(base_path)
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
        finally:
            hss.close()
        empty_default = cls()
        cp = subprocess.run(['docker', 'inspect', empty_default.container_name],
                            stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT,
                            universal_newlines=True)
        if cp.returncode == 0:
            findings['container'] = {
                'name': empty_default.container_name,
            }
        return findings


    async def launch_instance(self, instance_id, ws_send, conntype, publickey):
        self.instance_id = instance_id
        launch_args = ['docker', 'run', '-d',
                       '-h', self.container_name,
                       '--name', self.container_name,
                       '--device=/dev/net/tun:/dev/net/tun',
                       '--cap-add=NET_ADMIN']
        launch_args += ['hs.rerobots.net/generic:latest']
        subprocess.check_call(launch_args)
        self.status = 'READY'
        await ws_send(json.dumps({
            'v': 0,
            'cmd': 'INSTANCE_STATUS',
            's': self.status
        }))

    async def destroy_instance(self):
        destroy_args = ['docker', 'rm', '-f', self.container_name]
        subprocess.check_call(destroy_args)
