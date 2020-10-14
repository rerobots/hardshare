#!/usr/bin/env python
"""
SCL <scott@rerobots.net>
Copyright (c) 2020 rerobots, Inc.
"""
import os
import os.path
import tempfile

import pytest

from hardshare.mgmt import add_key, get_local_config
from hardshare.err import Error

from fixtures import RPUBLIC_KEY, RSECRET_KEY, api_token, config


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
