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
import logging.handlers
import os
import os.path
import subprocess
import sys

import yaml

from .core import WorkspaceInstance
from .mgmt import get_local_config, add_key, add_ssh_path, list_local_keys
from .mgmt import find_wd, modify_local
from .api import HSAPIClient


logger = logging.getLogger('hardshare')
logger.setLevel(logging.WARNING)
loghandler = logging.handlers.WatchedFileHandler(filename='hardshare_client.log', mode='a')
loghandler.setLevel(logging.DEBUG)
loghandler.setFormatter(logging.Formatter('%(name)s.%(funcName)s (%(levelname)s) (pid: {});'
                                          ' %(asctime)s ; %(message)s'
                                          .format(os.getpid())))
logger.addHandler(loghandler)


def get_config_with_index(id_prefix=None):
    try:
        config = get_local_config()
    except:
        print('error loading configuration data. does it exist?')
        return None, None, 1
    if len(config['wdeployments']) == 0:
        print(('ERROR: no workspace deployment in local configuration.'))
        return config, None, 1
    if id_prefix:
        index = find_wd(config, id_prefix)
        if index is None:
            print('ERROR: given prefix does not match precisely 1 workspace deployment')
            return config, None, 1
    else:
        if len(config['wdeployments']) > 1:
            print('ERROR: ambiguous command: more than 1 workspace deployment defined.')
            return config, None, 1
        index = 0
    return config, index, 0


def main(argv=None):
    if argv is None:
        argv = sys.argv[1:]
    argparser = argparse.ArgumentParser(description=('Command-line interface'
                                                     ' for the hardshare client'))
    argparser.add_argument('-V', '--version', action='store_true', default=False,
                           help='print version of hardshare (this) package.',
                           dest='print_version')
    argparser.add_argument('-v', '--verbose', action='store_true', default=False,
                           help='print verbose messages about actions by the hardshare client',
                           dest='verbose')
    argparser.add_argument('--format', metavar='FORMAT',
                           default=None, type=str,
                           help=('special output formatting (default is no special formatting); '
                                 'options: YAML , JSON'),
                           dest='output_format')
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
    help_parser = subparsers.add_parser('help', help='print this help message and exit')
    help_parser.add_argument('help_target_command', metavar='COMMAND', type=str, nargs='?')

    config_commanddesc = 'manage local and remote configuration'
    config_parser = subparsers.add_parser('config',
                                          description=config_commanddesc,
                                          help=config_commanddesc)
    config_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                               help=('id of workspace deployment for configuration changes'
                                     ' (can be unique prefix); '
                                     'this argument is not required '
                                     'if there is only 1 workspace deployment'))
    config_parser.add_argument('-c', '--create', action='store_true', default=False,
                               dest='create_config',
                               help='if no local configuration is found, then create one')
    config_parser.add_argument('--add-key', metavar='FILE',
                               dest='new_api_token',
                               help='add new account key')
    config_parser.add_argument('--add-ssh-path', metavar='PATH',
                               dest='new_ssh_path',
                               help='add path to SSH key pair (does NOT copy the key)')
    config_parser.add_argument('--add-raw-device', metavar='PATH', type=str,
                               dest='raw_device_path', default=None,
                               help='add device file to present in container')
    config_parser.add_argument('--assign-image', metavar='IMG', type=str,
                               dest='cprovider_img', default=None,
                               help='assign image for cprovider to use (advanced option)')
    config_parser.add_argument('--rm-raw-device', metavar='PATH', type=str,
                               dest='remove_raw_device_path', default=None,
                               help='remove device previously marked for inclusion in container')
    config_parser.add_argument('-p', '--prune', action='store_true', default=False,
                               dest='prune_err_keys',
                               help=('delete files in local key directory that'
                                     ' are not valid; to get list of'
                                     ' files with errors, try `--list`'))
    config_parser.add_argument('-l', '--list', action='store_true', default=False,
                               dest='list_config',
                               help='list configuration')
    config_parser.add_argument('--local', action='store_true', default=False,
                               dest='only_local_config',
                               help='only show local configuration data')
    config_parser.add_argument('--declare', metavar='ID',
                               dest='declared_wdeployment_id', default=None,
                               help=('declare that workspace deployment is'
                                     ' hosted here. (this only works if it'
                                     ' has been previously registered under'
                                     ' the same user account.)'))

    register_commanddesc = 'register new workspace deployment'
    register_parser = subparsers.add_parser('register',
                                            description=register_commanddesc,
                                            help=register_commanddesc)

    check_commanddesc = 'check registration of this workspace deployment'
    check_parser = subparsers.add_parser('check',
                                         description=check_commanddesc,
                                         help=check_commanddesc)
    check_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                              help=('id of workspace deployment to check'
                                    ' (can be unique prefix)'))

    dissolve_commanddesc = ('dissolve this workspace deployment, making it'
                            ' unavailable for any future use'
                            ' (THIS CANNOT BE UNDONE)')
    dissolve_parser = subparsers.add_parser('dissolve',
                                            description=dissolve_commanddesc,
                                            help=dissolve_commanddesc)
    dissolve_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                                 help=('id of workspace deployment to dissolve'
                                       ' (can be unique prefix)'))

    status_commanddesc = 'get status of local instances and daemon'
    status_parser = subparsers.add_parser('status',
                                          description=status_commanddesc,
                                          help=status_commanddesc)
    status_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                               help=('id of target workspace deployment'
                                     ' (can be unique prefix)'))

    advertise_commanddesc = 'advertise availability, accept new instances'
    advertise_parser = subparsers.add_parser('ad',
                                             description=advertise_commanddesc,
                                             help=advertise_commanddesc)
    advertise_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                                  help=('id of workspace deployment to advertise'
                                        ' (can be unique prefix); '
                                        'this argument is not required '
                                        'if there is only 1 workspace deployment'))
    advertise_parser.add_argument('-d', '--daemon', action='store_true', default=False,
                                  help='detach from invoking terminal (i.e., run as daemon)',
                                  dest='become_daemon')

    terminate_commanddesc = 'mark as unavailable; optionally wait for current instance to finish'
    terminate_parser = subparsers.add_parser('terminate',
                                             description=terminate_commanddesc,
                                             help=terminate_commanddesc)
    terminate_parser.add_argument('id_prefix', metavar='ID', nargs='?', default=None,
                                  help=('id of target workspace deployment'
                                        ' (can be unique prefix)'))
    terminate_parser.add_argument('-f', '--force', action='store_true', default=False,
                                  help=('if there is an active instance, then'
                                        ' stop it without waiting'),
                                  dest='force_terminate')
    help_message_purge = ('if the server indicates that an instance is active,'
                          ' but there is not one or it is otherwise in a'
                          ' non-recoverable state, then mark it remotely as'
                          ' terminated and attempt local clean-up; this'
                          ' command is a last resort. First, try `hardshare'
                          ' terminate` without --purge.')
    terminate_parser.add_argument('--purge', action='store_true', default=False,
                                  help=help_message_purge,
                                  dest='purge_supposed_instance')

    argv_parsed = argparser.parse_args(argv)

    if argv_parsed.print_version or argv_parsed.command == 'version':
        from . import __version__ as hardshare_pkg_version
        print(hardshare_pkg_version)
        return 0

    elif argv_parsed.command is None or argv_parsed.command == 'help':
        if argv_parsed.help_target_command is not None:
            if argv_parsed.help_target_command == 'config':
                config_parser.print_help()
            elif argv_parsed.help_target_command == 'register':
                register_parser.print_help()
            elif argv_parsed.help_target_command == 'check':
                check_parser.print_help()
            elif argv_parsed.help_target_command == 'dissolve':
                dissolve_parser.print_help()
            elif argv_parsed.help_target_command == 'status':
                status_parser.print_help()
            elif argv_parsed.help_target_command == 'ad':
                advertise_parser.print_help()
            elif argv_parsed.help_target_command == 'terminate':
                terminate_parser.print_help()
            else:
                argparser.print_help()
        else:
            argparser.print_help()
        return 0

    if argv_parsed.verbose:
        logger.setLevel(logging.DEBUG)

    if argv_parsed.output_format is not None:
        output_format = argv_parsed.output_format.lower()
        if output_format not in ['yaml', 'json']:
            print('output format unrecognized: {}'.format(argv_parsed.output_format))
            return 1
    else:
        output_format = None

    try:
        ac = HSAPIClient(server_name=argv_parsed.server_name,
                         server_port=argv_parsed.server_port,
                         verify_certs=(not argv_parsed.ignore_certs))
    except:
        ac = None

    if argv_parsed.command == 'status':
        try:
            config = get_local_config()
        except:
            print('error loading configuration data. does it exist?')
            return 1
        if argv_parsed.id_prefix is None:
            if len(config['wdeployments']) == 0:
                findings = [WorkspaceInstance.inspect_instance()]
            else:
                findings = []
                for wd in config['wdeployments']:
                    findings.append(WorkspaceInstance.inspect_instance(wdeployment=wd))
        else:
            findings = []
            for m in find_wd(config, argv_parsed.id_prefix, one_or_none=False):
                findings.append(WorkspaceInstance.inspect_instance(wdeployment=config['wdeployments'][m]))

        if output_format == 'json':
            print(json.dumps(findings))
        else:  # output_format == 'yaml'
            print(yaml.dump(findings, default_flow_style=False))

    elif argv_parsed.command == 'ad':
        if ac is None:
            print('cannot register without initial local configuration.'
                  ' (try `hardshare config --create`)')
            return 1
        config, index, rc = get_config_with_index(argv_parsed.id_prefix)
        if rc != 0:
            return rc
        if argv_parsed.become_daemon:
            if os.fork() != 0:
                return 0
            os.close(0)
            os.close(1)
            os.close(2)
        return ac.run_sync(config['wdeployments'][index]['id'])

    elif argv_parsed.command == 'terminate':
        config, index, rc = get_config_with_index(argv_parsed.id_prefix)
        if rc != 0:
            return rc
        if argv_parsed.purge_supposed_instance:
            cprovider = config['wdeployments'][index]['cprovider']
            if cprovider not in ['docker', 'podman']:
                print('unknown cprovider: {}'.format(cprovider))
                return 1
            findings = WorkspaceInstance.inspect_instance(wdeployment=config['wdeployments'][index])
            if 'container' in findings:
                try:
                    subprocess.check_call([cprovider, 'rm', '-f',
                                           findings['container']['name']],
                                          stdout=subprocess.DEVNULL,
                                          stderr=subprocess.DEVNULL)
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
            ac.terminate(config['wdeployments'][index]['id'])
            return 0

    elif argv_parsed.command == 'register':
        if ac is None:
            print('cannot register without initial local configuration.'
                  ' (try `hardshare config --create`)')
            return 1
        print(ac.register_new())

    elif argv_parsed.command == 'check':
        if ac is None:
            print('no local configuration found. (try `hardshare config -h`)')
            return 1
        try:
            res = ac.check_registration(argv_parsed.id_prefix)
        except:
            print('Error occurred while contacting remote server '
                  'at {}'.format(ac.base_uri))
            return 1
        if 'err' in res:
            if res['err'] == 'not found':
                print('not found: workspace deployment with id prefix {}'
                      .format(res['id_prefix']))
            elif res['err'] == 'wrong authorization token':
                print('wrong API token. Did it expire?')
            else:
                print(res['err'])
            return 1
        else:
            print('summary of workspace deployment {}'.format(res['id']))
            print('\tcreated: {}'.format(res['date_created']))
            print('\torigin (address) of registration: {}'.format(res['origin']))
            if 'date_dissolved' in res:
                print('\tdissolved: {}'.format(res['origin']))

    elif argv_parsed.command == 'dissolve':
        if ac is None:
            print('no local configuration found. (try `hardshare config -h`)')
            return 1
        try:
            res = ac.dissolve_registration(argv_parsed.id_prefix)
        except:
            print('Error occurred while contacting remote server '
                  'at {}'.format(ac.base_uri))
            return 1
        if 'err' in res:
            if res['err'] == 'not found':
                print('not found: workspace deployment with id prefix {}'
                      .format(res['id_prefix']))
            elif res['err'] == 'wrong authorization token':
                print('wrong API token. Did it expire?')
            else:
                print(res['err'])
            return 1

    elif argv_parsed.command == 'config':
        if argv_parsed.list_config:
            try:
                config = get_local_config(create_if_empty=argv_parsed.create_config,
                                          collect_errors=True)
            except:
                print('error loading configuration data.'
                      ' does it exist? is it broken?')
                return 1

            if not argv_parsed.only_local_config:
                # Try to get remote config, given possibly new local config
                try:
                    assert ac is not None
                    remote_config = ac.get_remote_config()
                except:
                    print('Error occurred while contacting remote server '
                          'at {}'.format(ac.base_uri))
                    return 1

                config = {
                    'local': config,
                    'remote': remote_config,
                }

            if output_format == 'json':
                print(json.dumps(config))

            elif output_format == 'yaml':
                print(yaml.dump(config, default_flow_style=False))

            else:
                if 'local' not in config:
                    config = {
                        'local': config,
                        'remote': None,
                    }
                print('workspace deployments defined in local configuration:')
                if len(config['local']['wdeployments']) == 0:
                    print('\t(none)')
                else:
                    for wdeployment in config['local']['wdeployments']:
                        print('{}\n\turl: {}\n\towner: {}\n\tcprovider: {}\n\tcargs: {}\n\timg: {}'.format(
                            wdeployment['id'],
                            'https://rerobots.net/workspace/{}'.format(wdeployment['id']),
                            wdeployment['owner'],
                            wdeployment['cprovider'],
                            wdeployment['cargs'],
                            wdeployment['image'],
                        ))

                print('\nfound keys:')
                if len(config['local']['keys']) == 0:
                    print('\t(none)')
                else:
                    print('\t' + '\n\t'.join(config['local']['keys']))
                if 'err_keys' in config['local'] and len(config['local']['err_keys']) > 0:
                    print('found possible keys with errors:')
                    for err_key_path, err in config['local']['err_keys'].items():
                        print('\t {}: {}'.format(err, err_key_path))

                if config['remote']:
                    if 'err' in config['remote']:
                        print('Error occurred while contacting remote server.')
                        if config['remote']['err'] == 'wrong authorization token':
                            print('wrong API token. Did it expire?')
                        else:
                            print(config['remote']['err'])
                        return 1
                    if len(config['remote']['deployments']) == 0:
                        print('\nno registered workspace deployments with this user account')
                    else:
                        print('\nregistered workspace deployments with this user account:')
                        for wd in config['remote']['deployments']:
                            print('{}'.format(wd['id']))
                            print('\tcreated: {}'.format(wd['date_created']))
                            print('\torigin (address) of registration: {}'
                                  .format(wd['origin']))

        elif argv_parsed.prune_err_keys:
            _, errored_keys = list_local_keys(collect_errors=True)
            for err_key_path, err in errored_keys.items():
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

        elif argv_parsed.declared_wdeployment_id is not None:
            assert ac is not None
            ac.declare_existing(argv_parsed.declared_wdeployment_id)
            ac.sync_config()

        elif argv_parsed.raw_device_path is not None:
            config, index, rc = get_config_with_index(argv_parsed.id_prefix)
            if rc != 0:
                return rc
            cprovider = config['wdeployments'][index]['cprovider']
            if cprovider not in ['docker', 'podman']:
                print('unknown cprovider: {}'.format(cprovider))
                return 1
            if not os.path.exists(argv_parsed.raw_device_path):
                print('ERROR: given device file does not exist')
                return 1
            carg = '--device={D}:{D}'.format(D=argv_parsed.raw_device_path)
            config['wdeployments'][index]['cargs'].append(carg)
            modify_local(config)

        elif argv_parsed.remove_raw_device_path is not None:
            config, index, rc = get_config_with_index(argv_parsed.id_prefix)
            if rc != 0:
                return rc
            carg = '--device={D}:{D}'.format(D=argv_parsed.remove_raw_device_path)
            config['wdeployments'][index]['cargs'].remove(carg)
            modify_local(config)

        elif argv_parsed.cprovider_img is not None:
            config, index, rc = get_config_with_index(argv_parsed.id_prefix)
            if rc != 0:
                return rc
            cprovider = config['wdeployments'][index]['cprovider']
            if cprovider not in ['docker', 'podman']:
                print('unknown cprovider: {}'.format(cprovider))
                return 1
            cp_images = subprocess.run([cprovider, 'image', 'exists', argv_parsed.cprovider_img])
            if cp_images.returncode != 0:
                print('ERROR: given image name is not recognized by cprovider')
                return 1
            config['wdeployments'][index]['image'] = argv_parsed.cprovider_img
            modify_local(config)

        else:
            print('Use `hardshare config` with a switch. For example, `hardshare config -l`')
            print('or to get a help message, enter\n\n    hardshare help config')
            return 1

    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
