#!/usr/bin/env python
"""Basic tests of the command-line interface (CLI)

SCL <scott@rerobots.net>
Copyright (c) 2018 rerobots, Inc.
"""
try:
    from cStringIO import StringIO
except ImportError:  # if Python 3
    from io import StringIO
import sys

import hardshare
from hardshare import cli


def test_version():
    original_stdout = sys.stdout
    sys.stdout = StringIO()
    cli.main(['version'])
    res = sys.stdout.getvalue().strip()
    sys.stdout = original_stdout
    assert hardshare.__version__ == res

def test_help():
    original_stdout = sys.stdout
    sys.stdout = StringIO()
    cli.main(['help'])
    res = sys.stdout.getvalue().strip()
    sys.stdout = original_stdout
    assert 'Command-line interface for the hardshare client' in res


if __name__ == '__main__':
    test_version()
