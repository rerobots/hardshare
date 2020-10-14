#!/usr/bin/env python
"""
SCL <scott@rerobots.net>
Copyright (c) 2020 rerobots, Inc.
"""
import os.path

import pytest

from hardshare.mgmt import get_local_config
from hardshare.err import Error


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
