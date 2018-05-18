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
"""Routines for Workspace deployment management
"""
import os
import os.path

import jwt


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
