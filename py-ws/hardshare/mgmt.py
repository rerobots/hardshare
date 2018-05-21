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


# TODO: this should eventually be placed in a public key store
WEBUI_PUBLIC_KEY = """ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC3ehAzPyTEsr1IKqwGAQY7uHIlsxYIwwTWghrnic32Z7uuvv89DuQh6xWtTYMk+cHm3jZQ/MqM1uj8D3EqN5QpjxGyYf1u2sxyNECYjpZD3qpcZlNxhCLJE/MsnLXzBjWBuF1IzItglcka3WURYY8vPkHuAedKd0OjB8jnWUlGGOHLLSnDlw78oJS8S8qB80vw5XGy2PfQnvz9AQrsn72E5egWabP3eGipWMsgOTEk830a1hOK+pu/5MvWI9TFl5Uq0UPEbQOqZTTy+ILChOwFimkxf3irIbfS1Gl9BJzwYnlxpQDVTlC5Gq3OgkWM6tbsUNPlhwgQiph84QRYsx2F scott@cero"""


def list_local_keys(collect_errors=False):
    """get list of local keys that appear valid

    If collect_errors=True, then also return list of pairs of the form
    <<path, reason>> where `path` (str) is the path of the file and
    `reason` (str) is a description of why it does not appear to be a
    valid key.

    The intent of the collect_errors parameter is to help identify
    stale keys and other noise in the local keys directory, which can
    be cleaned via `hardshare config -p`
    """
    base_path = '~/.rerobots'
    base_path = os.path.expanduser(base_path)
    if not os.path.exists(base_path):
        return []
    keys_dir = os.path.join(base_path, 'keys')
    if not os.path.exists(keys_dir):
        return []
    likely_keys = []
    if collect_errors:
        errored_keys = []
    for x in os.listdir(keys_dir):
        x = os.path.join(keys_dir, x)
        try:
            with open(x) as fp:
                jwt.decode(fp.read(),
                           issuer='rerobots.net',
                           audience='rerobots.net',
                           key=WEBUI_PUBLIC_KEY,
                           algorithm='RS256')
            x_perm = os.stat(x).st_mode & 511
            if x_perm == 0o600 or x_perm == 0o400:
                likely_keys.append(x)
        except jwt.exceptions.DecodeError:
            if collect_errors:
                errored_keys.append((x, 'coding error'))
        except jwt.exceptions.ImmatureSignatureError:
            if collect_errors:
                errored_keys.append((x, 'immature signature'))
        except jwt.exceptions.InvalidIssuerError:
            if collect_errors:
                errored_keys.append((x, 'invalid issuer'))
        except jwt.exceptions.InvalidAudienceError:
            if collect_errors:
                errored_keys.append((x, 'invalid audience'))
        except jwt.exceptions.ExpiredSignatureError:
            if collect_errors:
                errored_keys.append((x, 'expired signature'))
        except:
            if collect_errors:
                errored_keys.append((x, 'unknown error'))
    if collect_errors:
        return likely_keys, errored_keys
    else:
        return likely_keys


def get_config(create_if_empty=False, collect_errors=False):
    base_path = '~/.rerobots'
    base_path = os.path.expanduser(base_path)
    if not os.path.exists(base_path):
        if create_if_empty:
            os.makedirs(os.path.join(base_path, 'keys'))
        else:
            raise Error('no configuration data found')
    keys, errored_keys = list_local_keys(collect_errors=collect_errors)
    config = {
        'keys': keys,
        'err_keys': errored_keys,
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
