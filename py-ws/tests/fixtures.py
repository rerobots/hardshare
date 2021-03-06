"""
SCL <scott@rerobots.net>
Copyright (c) 2020 rerobots, Inc.
"""
import os.path
import time

import jwt

import pytest

from hardshare.mgmt import get_local_config, modify_local


RPUBLIC_KEY = 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCZ7sEpV8VQ+mLYNbfmUIbttQgXSm6vie2Vh1q+nfiEnZITxUhWtcEg6faWxjdpi4v3ZQMUidgUkAcQRq8bqvJ7kbKLXD5ElAVYozeYlogSbb9odZrtRcrTL+GrWXGyk3S+aBQ/craU39DvJ6TXSmJbBvy6LmoLn424DFn0HXM/zzh8ZOCbbvuvdKHyOUCWdX80rdw9GLx25RWE8C/Pgk361LLZHqjIC3xsBqiCACRN2MGv4AbKUVAiveVEjhpxnQ3dFzktoqYGfKaBqITxMtOf9dOHuaJ15yxcAP77QQ3mXqm3/yTVT33QbaAqa/GUCtKckzGqoA098a6vKzUzoFFwwR3nidvcr36iwLpmofzu3xybx/xAyxpziedrWe/XxfvEm74s9XO1gndgeKhkOObATK1hJn94B+yrYGycLhdME8oMpx3Ty2sAaQH5WfEQ741J2JozEkpL/1aoN1AmVo7knBkWIgPNyFJZAcJjJdD5gieoN0oUDf02H5Rtsm/jK6k= scott@cero'

RSECRET_KEY = '''-----BEGIN RSA PRIVATE KEY-----
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
-----END RSA PRIVATE KEY-----
'''


@pytest.fixture
def config(tmpdir, monkeypatch):
    tmpdir_name = str(tmpdir)
    def mock_expanduser(path):
        return path.replace('~', tmpdir_name)
    monkeypatch.setattr(os.path, 'expanduser', mock_expanduser)
    return get_local_config(create_if_empty=True)


@pytest.fixture
def api_token(monkeypatch):
    import hardshare.mgmt
    monkeypatch.setattr(hardshare.mgmt, 'WEBUI_PUBLIC_KEY', RPUBLIC_KEY)
    creationtime = int(time.time())
    payload = {
        'sub': 'username',
        'iss': 'rerobots.net',
        'aud': 'rerobots.net',
        'exp': creationtime + 10,
        'nbf': creationtime - 1
    }
    tok = jwt.encode(payload, key=RSECRET_KEY, algorithm='RS256')
    return str(tok, encoding='utf-8')


@pytest.fixture
def config_with_wd(tmpdir, monkeypatch):
    tmpdir_name = str(tmpdir)
    def mock_expanduser(path):
        return path.replace('~', tmpdir_name)
    monkeypatch.setattr(os.path, 'expanduser', mock_expanduser)
    config = get_local_config(create_if_empty=True)
    config['wdeployments'].append({
        'id': 'ae064a41-065e-426f-9a1e-619fd2d33fb6',
        'owner': 'username',
    })
    modify_local(config)
    return get_local_config()
