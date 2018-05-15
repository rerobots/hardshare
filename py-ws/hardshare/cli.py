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
"""Command-line interface
"""
import argparse
import sys


def main(argv=None):
    aparser = argparse.ArgumentParser(description=('Command-line interface'
                                                   ' for the hardshare client'))
    aparser.add_argument('-V', '--version', action='store_true', default=False,
                         help='print version of hardshare (this) package.',
                         dest='print_version')
    argv_parsed = aparser.parse_args(argv)

    if argv_parsed.print_version:
        from . import __version__ as hardshare_pkg_version
        print(hardshare_pkg_version)
        return 0

    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
