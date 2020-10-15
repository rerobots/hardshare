#!/usr/bin/env python
"""
SCL <scott@rerobots.net>
Copyright (c) 2020 rerobots, Inc.
"""
import os
import os.path
import tempfile

import pytest

from hardshare.mgmt import add_key, find_wd, get_local_config, rm_wd
from hardshare.err import Error

from fixtures import RPUBLIC_KEY, RSECRET_KEY, api_token, config, config_with_wd


def test_get_local_config(tmpdir, monkeypatch):
    tmpdir_name = str(tmpdir)
    def mock_expanduser(path):
        return path.replace('~', tmpdir_name)
    monkeypatch.setattr(os.path, 'expanduser', mock_expanduser)

    with pytest.raises(Error):
        get_local_config()

    config = get_local_config(create_if_empty=True)
    assert 'wdeployments' in config
    assert len(config['wdeployments']) == 0
    assert 'keys' in config
    assert len(config['keys']) == 0


def test_add_key(config, api_token):
    assert len(config['keys']) == 0

    fd, fname = tempfile.mkstemp()
    os.write(fd, b'not valid')
    os.close(fd)
    add_key(fname)
    config = get_local_config(collect_errors=True)
    assert len(config['keys']) == 0
    assert len(config['err_keys']) == 1
    err_path = list(config['err_keys'].keys())[0]
    assert config['err_keys'][err_path] == 'coding error'
    os.remove(err_path)
    config = get_local_config(collect_errors=True)
    assert len(config['err_keys']) == 0

    fd, fname = tempfile.mkstemp()
    os.write(fd, api_token.encode())
    os.close(fd)
    add_key(fname)
    config = get_local_config()
    assert len(config['keys']) == 1


@pytest.mark.parametrize('id_prefix, expected', [
    ('ae064a41-065e-426f-9a1e-619fd2d33fb6', 0),
    ('ae064a41', 0),
    ('a', 0),
    ('b', None),
])
def test_find_wd(config_with_wd, id_prefix, expected):
    assert find_wd(config_with_wd, id_prefix) == expected


@pytest.mark.parametrize('id_prefix, should_find', [
    ('ae064a41-065e-426f-9a1e-619fd2d33fb6', True),
    ('ae064a41', True),
    ('a', True),
    ('b', False),
])
def test_rm_wd(config_with_wd, id_prefix, should_find):
    rm_wd(config_with_wd, id_prefix, save=True)

    # check copy in memory
    assert len(config_with_wd['wdeployments']) == (0 if should_find else 1)

    # re-read from file
    config = get_local_config()
    assert len(config['wdeployments']) == (0 if should_find else 1)
