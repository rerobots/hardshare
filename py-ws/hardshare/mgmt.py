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
import json
import logging
import os
import os.path
import stat

import jwt

from .err import Error


logger = logging.getLogger(__name__)


# TODO: this should eventually be placed in a public key store
WEBUI_PUBLIC_KEY = """ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAACAQCpaC+WZHknryjjioYHcCpVweekg7lTFoy4Qc+je5YTJwCEt9NuEQis0Y5pGkVVNWCVTsrA95Cq/H67Sls5dvxul/R20hzmKVaWBxN6QIHxJAqB7z454pNvGRrLOi/PLBeImNgvOC4d5L4uYQyFVpBIRBSbRNaqV9zbfSIFFBjfqIv6Cp4SvvFu0rtHw4A/H62Firy+sap5nkcTynOSe3OskSItmAQT5I42nN6QfjKf9Y2MY0iFbP4GixGdvi0Nre0lRqVV8h5OQwGcrzfSQKBAeWASmJ2fyXja/WBk0UZFx0GSfNEP201xJbx2b9TVZMekBvJpYyDzc2nR/1kddLWucvE3malTW7Geuih5/hjPALXsqFL/zDZ89AF3DsY/QX2cSFazXBVVUz/UdSCmddZMtnL589sWNB2DkUldJXLkCNxGKlKh9aWoH/a6b3AR0EU67RyKK0wSMhcBPGcHLa2Zz/iaFJyNfEmeEhZ8KFz0oZ34cLEwtQRHYI7LtjT/ZT/5AamkGldQ4lMAFOdfc1Qu2LeOpYT9FOF+mkuT7MMlCpWNBR48+AVjni+uoNymStYBDapxPHDJ2D9TjgMDzKgBl+VqwNag8A532dgBxIZsjrqfMMQC7VTUVoeRXgOM8D0FZWYRmw6ozrYsJVaLE7OSqi9JKi50fFCoAMsbRKeFZw== scott@cero"""


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
    base_path = os.path.join(os.path.expanduser('~'), '.rerobots')
    likely_keys = []
    if collect_errors:
        errored_keys = []
    if not os.path.exists(base_path):
        if collect_errors:
            return likely_keys, errored_keys
        else:
            return likely_keys
    keys_dir = os.path.join(base_path, 'keys')
    if not os.path.exists(keys_dir):
        if collect_errors:
            return likely_keys, errored_keys
        else:
            return likely_keys
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
            elif collect_errors:
                errored_keys.append((x, 'unsafe file permissions'))
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


def get_local_config(create_if_empty=False, collect_errors=False):
    base_path = os.path.join(os.path.expanduser('~'), '.rerobots')
    if not os.path.exists(base_path):
        if create_if_empty:
            os.makedirs(os.path.join(base_path, 'keys'))
        else:
            raise Error('no configuration data found')
    path = os.path.join(base_path, 'main')
    logger.debug('full path to local config file: {}'.format(path))
    if not os.path.exists(path):
        if create_if_empty:
            logger.debug('local config file not found. creating new one...')
            with open(path, 'wt') as fp:
                fp.write('{"version": 0, "wdeployments": []}')
        else:
            msg = 'get_local_config(): no configuration data found'
            logger.error(msg)
            raise Error(msg)
    with open(path) as fp:
        config = json.load(fp)
    assert 'version' in config and config['version'] == 0
    assert 'wdeployments' in config
    if collect_errors:
        keys, errored_keys = list_local_keys(collect_errors=True)
    else:
        keys = list_local_keys(collect_errors=False)
    config.update({
        'keys': keys,
    })
    if collect_errors:
        config['err_keys'] = errored_keys
    return config


def modify_local(config):
    """update local (main) configuration

    This function ignores unrecognized keys in the given dict. Any
    such extra data are not saved.
    """
    assert 'version' in config and config['version'] == 0
    assert 'wdeployments' in config
    base_path = os.path.join(os.path.expanduser('~'), '.rerobots')
    with open(os.path.join(base_path, 'main')) as fp:
        prev_config = json.load(fp)
    assert 'version' in prev_config and prev_config['version'] == 0
    assert 'wdeployments' in prev_config
    new_config = {
        'version': 0,
        'wdeployments': [],
    }
    if 'ssh_key' in config:
        new_config['ssh_key'] = config['ssh_key']
    for wd in config['wdeployments']:
        new_config['wdeployments'].append({
            'id': wd['id'],
            'owner': wd['owner'],
            'cprovider': 'docker',
            'image': "hs.rerobots.net/generic",
            "container_name": "rrc",
        })
    with open(os.path.join(base_path, 'main'), 'wt') as fp:
        json.dump(new_config, fp)


def add_key(path, create_if_empty=False):
    """save rerobots API token into local configuration

    If successful, this function moves the file into the local
    configuration directory (default: ~/.rerobots).
    """
    base_path = os.path.join(os.path.expanduser('~'), '.rerobots')
    config = get_local_config(create_if_empty=create_if_empty)
    newkey_basename = os.path.basename(path)
    existing_basenames = [os.path.basename(keypath) for keypath in config['keys']]
    if newkey_basename in existing_basenames:
        newkey_basename += '-' + datetime.utcnow().strftime('%Y%m%d-%H%M%S')
    assert not os.path.exists(os.path.join(base_path, 'keys', newkey_basename))
    os.rename(os.path.join(os.path.dirname(path), newkey_basename),
              os.path.join(base_path, 'keys', newkey_basename))
    os.chmod(os.path.join(base_path, 'keys', newkey_basename), mode=stat.S_IRUSR|stat.S_IWUSR)


def add_ssh_path(path, create_if_empty=False):
    """save path to SSH private key for SSH tunnel based instances

    This function checks that the path exists and has appropriate file
    permissions for use as a private key with SSH. However, the file
    itself is never read by the hardshare client; only the path to it
    is saved in the local hardshare configuration.

    This function also checks that a file with path obtained by
    appending ".pub" exists. It is used as the public key.

    For SSH tunnel based instances (also referred to as `sshtun`),

    1. the public key is sent to rerobots-operated tunnel hubs,
       through which users are routed if they access an instance
       running on your hardware;

    2. the private key is read by the system `ssh` program, which is
       invoked as a subprocess. (Usage is entirely similar to calling
       `ssh -i PATH` from the command-line.)
    """
    path = os.path.expanduser(path)
    assert os.path.exists(path)
    assert os.path.exists(path + '.pub')
    permissions = os.stat(path).st_mode & 511
    assert permissions == 0o600 or permissions == 0o400
    config = get_local_config(create_if_empty=create_if_empty)
    config['ssh_key'] = path
    modify_local(config)
