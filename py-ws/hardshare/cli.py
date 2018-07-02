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
import json
import logging
import os
import subprocess
import sys

from .core import WorkspaceInstance
from .mgmt import get_local_config, add_key, add_ssh_path, list_local_keys
from .api import HSAPIClient


logger = logging.getLogger('hardshare')
logger.setLevel(logging.DEBUG)
loghandler = logging.StreamHandler()
loghandler.setLevel(logging.DEBUG)
loghandler.setFormatter(logging.Formatter('%(name)s (%(levelname)s) (pid: {}); %(asctime)s ; %(message)s'.format(os.getpid())))
logger.addHandler(loghandler)


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
    argparser.add_argument('--port', default=443,
                           help='port number of hardshare server',
                           dest='server_port')
    argparser.add_argument('-k', '--insecure', action='store_true', default=False,
                           help=('communications with hardshare servers always use TLS.'
                                 ' this switch causes certificates to not be verified.'),
                           dest='ignore_certs')

    subparsers = argparser.add_subparsers(dest='command')

    subparsers.add_parser('version', help='print version number and exit.')
    subparsers.add_parser('help', help='print this help message and exit')

    config_commanddesc = 'manage local and remote configuration'
    config_parser = subparsers.add_parser('config',
                                          description=config_commanddesc,
                                          help=config_commanddesc)
    config_parser.add_argument('-c', '--create', action='store_true', default=False,
                               dest='create_config',
                               help='if no local configuration is found, then create one')
    config_parser.add_argument('--add-key', metavar='FILE',
                               dest='new_api_token',
                               help='add new account key')
    config_parser.add_argument('--add-ssh-path', metavar='PATH',
                               dest='new_ssh_path',
                               help='add path to SSH key pair (does NOT copy the key)')
    config_parser.add_argument('-p', '--prune', action='store_true', default=False,
                               dest='prune_err_keys',
                               help='delete files in local key directory that are not valid; to get list of files with errors, try `--list`')
    config_parser.add_argument('-l', '--list', action='store_true', default=False,
                               dest='list_config',
                               help='list configuration')
    config_parser.add_argument('--local', action='store_true', default=False,
                               dest='only_local_config',
                               help='only show local configuration data')

    register_commanddesc = 'register new workspace deployment'
    register_parser = subparsers.add_parser('register',
                                            description=register_commanddesc,
                                            help=register_commanddesc)

    check_commanddesc = 'check registration of this workspace deployment'
    check_parser = subparsers.add_parser('check',
                                         description=check_commanddesc,
                                         help=check_commanddesc)
    check_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                              help='id of workspace deployment to check (can be unique prefix)')

    status_commanddesc = 'get status of local instances and daemon'
    status_parser = subparsers.add_parser('status',
                                          description=status_commanddesc,
                                          help=status_commanddesc)

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
    terminate_parser.add_argument('--purge', action='store_true', default=False,
                                  help='if the server thinks that there should be an instance active, but there is not one or it is otherwise in a non-recoverable state, then mark it remotely as terminated and attempt local clean-up; this command is a last resort. First, try `hardshare terminate` without --purge.',
                                  dest='purge_supposed_instance')

    argv_parsed = argparser.parse_args(argv)

    try:
        ac = HSAPIClient(server_name=argv_parsed.server_name,
                         server_port=argv_parsed.server_port,
                         verify_certs=(not argv_parsed.ignore_certs))
    except:
        ac = None

    if argv_parsed.print_version or argv_parsed.command == 'version':
        from . import __version__ as hardshare_pkg_version
        print(hardshare_pkg_version)

    elif argv_parsed.command is None or argv_parsed.command == 'help':
        argparser.print_help()

    elif argv_parsed.command == 'status':
        try:
            config = get_local_config(collect_errors=True)
        except:
            print('error loading configuration data. does it exist?')
            return 1
        findings = WorkspaceInstance.inspect_instance()
        print(json.dumps(findings))

    elif argv_parsed.command == 'ad':
        if ac is None:
            print('cannot register without initial local configuration. (try `hardshare config --create`)')
            return 1
        if argv_parsed.become_daemon:
            if os.fork() != 0:
                return 0
        ac.run_sync()

    elif argv_parsed.command == 'terminate':
        if argv_parsed.purge_supposed_instance:
            findings = WorkspaceInstance.inspect_instance()
            if 'container' in findings:
                try:
                    subprocess.check_call(['docker', 'rm', '-f',
                                           findings['container']['name']])
                except:
                    print('failed to stop container `{}`'.format(findings['container']['name']))
                    return 1
                return 0
            else:
                print('failed to detect local instance')
                return 1
        else:
            if ac is None:
                print('cannot terminate without valid API client')
                return 1
            ac.terminate()
            return 0

    elif argv_parsed.command == 'register':
        if ac is None:
            print('cannot register without initial local configuration. (try `hardshare config --create`)')
            return 1
        print(ac.register_new())

    elif argv_parsed.command == 'check':
        if ac is None:
            print('no local configuration found. (try `hardshare config -h`)')
            return 1
        res = ac.check_registration(argv_parsed.id_prefix)
        if 'err' in res and res['err'] == 'not found':
            print('not found: workspace deployment with id prefix {}'.format(res['id_prefix']))
            return 1
        else:
            print('summary of workspace deployment {}'.format(res['id']))
            print('\tcreated: {}'.format(res['date_created']))
            print('\torigin (address) of registration: {}'.format(res['origin']))

    elif argv_parsed.command == 'config':
        if argv_parsed.list_config:
            try:
                config = get_local_config(create_if_empty=argv_parsed.create_config, collect_errors=True)
            except:
                print('error loading configuration data. does it exist? is it broken?')
                return 1

            print('workspace deployments defined in local configuration:')
            if len(config['wdeployments']) == 0:
                print('\t(none)')
            else:
                for wdeployment in config['wdeployments']:
                    print('\t{} (owner: {})'.format(wdeployment['id'], wdeployment['owner']))

            print('found keys:')
            if len(config['keys']) == 0:
                print('\t(none)')
            else:
                print('\t' + '\n\t'.join(config['keys']))
            if 'err_keys' in config and len(config['err_keys']) > 0:
                print('found possible keys with errors:')
                for err_key_path, err in config['err_keys']:
                    print('\t {}: {}'.format(err, err_key_path))

            if not argv_parsed.only_local_config:
                # Try to get remote config, given possibly new local config
                ac = HSAPIClient(server_name=argv_parsed.server_name,
                                 server_port=argv_parsed.server_port,
                                 verify_certs=(not argv_parsed.ignore_certs))
                remote_config = ac.get_remote_config()
                if len(remote_config['deployments']) == 0:
                    print('no registered workspace deployments with this user account')
                else:
                    print('registered workspace deployments with this user account:')
                    for wd in remote_config['deployments']:
                        print('{}'.format(wd['id']))
                        print('\tcreated: {}'.format(wd['date_created']))
                        print('\torigin (address) of registration: {}'.format(wd['origin']))

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

        elif argv_parsed.new_ssh_path:
            try:
                add_ssh_path(argv_parsed.new_ssh_path)
            except:
                print('ERROR: {} or {} does not exist or '
                      'has the wrong permissions.'.format(
                          argv_parsed.new_ssh_path,
                          argv_parsed.new_ssh_path + '.pub'
                      ))
                return 1

        elif argv_parsed.create_config:
            get_local_config(create_if_empty=True)

    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
