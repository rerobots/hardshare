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


def test_alternative_version_spellings():
    original_stdout = sys.stdout

    # `version`
    sys.stdout = StringIO()
    cli.main(['version'])
    res_version = sys.stdout.getvalue().strip()

    # `-V`
    sys.stdout = StringIO()
    cli.main(['-V'])
    res_dashV = sys.stdout.getvalue().strip()

    # `--version`
    sys.stdout = StringIO()
    cli.main(['--version'])
    res_dashdashversion = sys.stdout.getvalue().strip()

    sys.stdout = original_stdout

    assert res_version == res_dashV
    assert res_version == res_dashdashversion


def test_help():
    original_stdout = sys.stdout
    sys.stdout = StringIO()
    cli.main(['help'])
    res = sys.stdout.getvalue().strip()
    sys.stdout = original_stdout
    assert 'Command-line interface for the hardshare client' in res


def test_alternative_help_spellings():
    original_stdout = sys.stdout

    # `help`
    sys.stdout = StringIO()
    cli.main(['help'])
    res_help = sys.stdout.getvalue().strip()

    # `-h`
    sys.stdout = StringIO()
    cli.main(['-h'])
    res_dashh = sys.stdout.getvalue().strip()

    # `--help`
    sys.stdout = StringIO()
    cli.main(['--help'])
    res_dashdashhelp = sys.stdout.getvalue().strip()

    sys.stdout = original_stdout

    assert res_help == res_dashh
    assert res_help == res_dashdashhelp


if __name__ == '__main__':
    test_version()
