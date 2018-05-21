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
import os
import sys

from .mgmt import get_config, add_key, list_local_keys


def main(argv=None):
    if argv is None:
        argv = sys.argv[1:]
    argparser = argparse.ArgumentParser(description=('Command-line interface'
                                                     ' for the hardshare client'))
    argparser.add_argument('-V', '--version', action='store_true', default=False,
                           help='print version of hardshare (this) package.',
                           dest='print_version')
    argparser.add_argument('-s', '--server-name', default='hs.rerobots.net',
                           help='name or IP address of hardshare server',
                           dest='server_name')
    argparser.add_argument('-k', '--insecure', action='store_true', default=False,
                           help=('communications with hardshare servers always use TLS.'
                                 ' this switch causes certificates to not be verified.'),
                           dest='ignore_certs')

    subparsers = argparser.add_subparsers(dest='command')

    subparsers.add_parser('version', help='print version number and exit.')
    subparsers.add_parser('help', help='print this help message and exit')

    config_commanddesc = 'manage local configuration'
    config_parser = subparsers.add_parser('config',
                                          description=config_commanddesc,
                                          help=config_commanddesc)
    config_parser.add_argument('-c', '--create', action='store_true', default=False,
                               dest='create_config',
                               help='if no local configuration is found, then create one')
    config_parser.add_argument('--add-key', metavar='FILE',
                               dest='new_api_token',
                               help='add new account key')
    config_parser.add_argument('-p', '--prune', action='store_true', default=False,
                               dest='prune_err_keys',
                               help='delete files in local key directory that are not valid; to get list of files with errors, try `--list`')
    config_parser.add_argument('-l', '--list', action='store_true', default=False,
                               dest='list_config',
                               help='list configuration')

    register_commanddesc = 'register new workspace deployment'
    register_parser = subparsers.add_parser('register',
                                            description=register_commanddesc,
                                            help=register_commanddesc)

    check_commanddesc = 'check registration of this workspace deployment'
    check_parser = subparsers.add_parser('check',
                                         description=check_commanddesc,
                                         help=check_commanddesc)

    advertise_commanddesc = 'advertise availability, accept new instances'
    advertise_parser = subparsers.add_parser('ad',
                                             description=advertise_commanddesc,
                                             help=advertise_commanddesc)
    advertise_parser.add_argument('-d', '--daemon', action='store_true', default=False,
                                  help='detach from invoking terminal (i.e., run as daemon)',
                                  dest='become_daemon')

    terminate_commanddesc = 'mark as unavailable; optionally wait for current instance to finish'
    terminate_parser = subparsers.add_parser('terminate',
                                             description=terminate_commanddesc,
                                             help=terminate_commanddesc)
    terminate_parser.add_argument('-f', '--force', action='store_true', default=False,
                                  help='if there is an active instance, then stop it without waiting',
                                  dest='force_terminate')

    argv_parsed = argparser.parse_args(argv)

    if argv_parsed.print_version or argv_parsed.command == 'version':
        from . import __version__ as hardshare_pkg_version
        print(hardshare_pkg_version)


    elif argv_parsed.command is None or argv_parsed.command == 'help':
        argparser.print_help()

    elif argv_parsed.command == 'config':
        if argv_parsed.list_config:
            try:
                config = get_config(create_if_empty=argv_parsed.create_config, collect_errors=True)
            except:
                print('error loading configuration data. does it exist?')
                return 1
            print('found keys:')
            print('\t' + '\n\t'.join(config['keys']))
            if len(config['err_keys']) > 0:
                print('found possible keys with errors:')
                for err_key_path, err in config['err_keys']:
                    print('\t {}: {}'.format(err, err_key_path))

        elif argv_parsed.prune_err_keys:
            _, errored_keys = list_local_keys(collect_errors=True)
            for err_key_path, err in errored_keys:
                print('deleting {}...'.format(err_key_path))
                os.unlink(err_key_path)

        elif argv_parsed.new_api_token:
            try:
                add_key(argv_parsed.new_api_token)
            except:
                print('failed to add key')
                return 1

        elif argv_parsed.create_config:
            get_config(create_if_empty=True)

    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
