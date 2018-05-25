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
import requests

from . import mgmt
from .err import Error


class HSAPIClient:
    def __init__(self, server_name='hs.rerobots.net', server_port=443, verify_certs=True):
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
