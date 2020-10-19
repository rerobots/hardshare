#!/usr/bin/env python
"""
SCL <scott@rerobots.net>
Copyright (c) 2020 rerobots, Inc.
"""
import os
import tempfile

import aiohttp

import pytest

from hardshare.api import HSAPIClient
from hardshare.mgmt import add_key, get_local_config
from hardshare.err import Error

from fixtures import config, api_token


class MockResponse:
    def __init__(self, status=200, payload=None):
        self.status = status
        self.payload = payload

    async def json(self):
        return self.payload

class MockClientSession:
    def __init__(self, headers):
        self.headers = headers

    async def get(self, url):
        if self.headers is None or 'Authorization' not in self.headers:
            return MockResponse(status=400, payload={'error_message': 'wrong authorization token'})
        if url.startswith('https://api.rerobots.net/hardshare/list'):
            payload = {
                'attr': {
                    'c42b5b73-376a-4c84-a20c-be865ca424c0': {
                        'desc': 'fake',
                    },
                    '57d3984a-f0bd-40af-a4ae-d322ae3a228c': {
                        'desc': 'other test',
                    },
                }
            }
            return MockResponse(status=200, payload=payload)
        elif url.endswith('/list?with_dissolved'):
            payload = {'owner': 'username', 'deployments': [
                {'id': 'c42b5b73-376a-4c84-a20c-be865ca424c0',
                 'date_created': '2020-04-08 10:30 UTC',
                 'origin': '76.126.210.153',
                 'dissolved': '2020-09-01 23:16 UTC'},
                {'id': '57d3984a-f0bd-40af-a4ae-d322ae3a228c',
                 'date_created': '2020-09-10 05:25 UTC',
                 'origin': '76.126.210.153',
                 'dissolved': None}
            ]}
            return MockResponse(status=200, payload=payload)
        elif url.endswith('/list'):
            payload = {'owner': 'username', 'deployments': [
                {'id': '57d3984a-f0bd-40af-a4ae-d322ae3a228c',
                 'date_created': '2020-09-10 05:25 UTC',
                 'origin': '76.126.210.153',
                 'dissolved': None}
            ]}
            return MockResponse(status=200, payload=payload)
        else:
            return MockResponse(status=404)

    async def post(self, url):
        if self.headers is None or 'Authorization' not in self.headers:
            return MockResponse(status=400, payload={'error_message': 'wrong authorization token'})
        payload = {
            'owner': 'username',
            'id': 'c42b5b73-376a-4c84-a20c-be865ca424c0',
        }
        return MockResponse(status=200, payload=payload)

    async def close(self):
        pass


def test_get_remote_config(config, api_token, monkeypatch):
    monkeypatch.setattr(aiohttp, 'ClientSession', MockClientSession)

    # no API token
    ac = HSAPIClient()
    remote_config = ac.get_remote_config()
    assert len(remote_config) == 1 and remote_config['err'] == 'wrong authorization token'

    # add API token
    fd, fname = tempfile.mkstemp()
    os.write(fd, api_token.encode())
    os.close(fd)
    add_key(fname)

    # valid API token
    ac = HSAPIClient()
    remote_config = ac.get_remote_config()
    assert remote_config['owner'] == 'username'
    assert 'deployments' in remote_config
    assert len(remote_config['deployments']) == 2

    remote_config = ac.get_remote_config(include_dissolved=False)
    assert len(remote_config['deployments']) == 1


def test_register_new(config, api_token, monkeypatch):
    monkeypatch.setattr(aiohttp, 'ClientSession', MockClientSession)

    # no API token
    ac = HSAPIClient()
    with pytest.raises(Error):
        ac.register_new()

    # add API token
    fd, fname = tempfile.mkstemp()
    os.write(fd, api_token.encode())
    os.close(fd)
    add_key(fname)

    # valid API token
    ac = HSAPIClient()
    wdid = ac.register_new()

    config = get_local_config()
    assert len(config['wdeployments']) == 1
    assert config['wdeployments'][0]['id'] == wdid

    with pytest.raises(Error):
        ac.register_new(at_most_one=True)
