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
"""Management routines for workspace deployments
"""
from datetime import datetime
import os
import os.path

import jwt

from .err import Error


def list_local_keys():
    base_path = '~/.rerobots'
    base_path = os.path.expanduser(base_path)
    if not os.path.exists(base_path):
        return []
    keys_dir = os.path.join(base_path, 'keys')
    if not os.path.exists(keys_dir):
        return []
    likely_keys = []
    for x in os.listdir(keys_dir):
        x = os.path.join(keys_dir, x)
        try:
            with open(x) as fp:
                jwt.decode(fp.read(), verify=False)
            x_perm = os.stat(x).st_mode & 511
            if x_perm == 0o600 or x_perm == 0o400:
                likely_keys.append(x)
        except:
            pass
    return likely_keys


def get_config(create_if_empty=False):
    base_path = '~/.rerobots'
    base_path = os.path.expanduser(base_path)
    if not os.path.exists(base_path):
        if create_if_empty:
            os.makedirs(os.path.join(base_path, 'keys'))
        else:
            raise Error('no configuration data found')
    config = {
        'keys': list_local_keys()
    }
    return config


def add_key(path, create_if_empty=False):
    base_path = '~/.rerobots'
    base_path = os.path.expanduser(base_path)
    config = get_config(create_if_empty=create_if_empty)
    newkey_basename = os.path.basename(path)
    existing_basenames = [os.path.basename(keypath) for keypath in config['keys']]
    if newkey_basename in existing_basenames:
        newkey_basename += '-' + datetime.utcnow().strftime('%Y%m%d-%H%M%S')
    assert not os.path.exists(os.path.join(base_path, 'keys', newkey_basename))
    os.rename(os.path.join(os.path.dirname(path), newkey_basename),
              os.path.join(base_path, 'keys', newkey_basename))
