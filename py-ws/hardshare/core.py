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
import subprocess


class WorkspaceInstance:
    def __init__(self):
        self.status = None
        self.container_name = 'rrc'

    async def launch_instance(self, ws_send, conntype, publickey):
        launch_args = ['docker', 'run', '-d',
                       '-h', self.container_name,
                       '--name', self.container_name,
                       '--device=/dev/net/tun:/dev/net/tun',
                       '--cap-add=NET_ADMIN']
        launch_args += ['hs.rerobots.net/generic:latest']
        subprocess.check_call(launch_args)

    async def destroy_instance(self):
        destroy_args = ['docker', 'rm', '-f', self.container_name]
        subprocess.check_call(destroy_args)
