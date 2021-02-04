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
from setuptools import setup


# Version of this package
MAJOR=0
MINOR=10
PATCH=2
devel=True

version = '{}.{}.{}'.format(MAJOR, MINOR, PATCH)
if devel:
    version += '.dev0'


if __name__ == '__main__':
    with open('README.rst') as fp:
        long_description = fp.read()

    with open('hardshare/_version.py', 'w') as fp:
        fp.write('''# This file was automatically generated by setup.py. Do not edit.
__version__ = '{}'
'''.format(version))

    setup(name='hardshare',
          version=version,
          author='Scott C. Livingston',
          author_email='q@rerobots.net',
          url='https://github.com/rerobots/hardshare',
          description='client (CLI and daemon) for the hardshare protocol',
          license='Apache 2.0',
          long_description=long_description,
          classifiers=['License :: OSI Approved :: Apache Software License',
                       'Programming Language :: Python :: 3',
                       'Programming Language :: Python :: 3.5',
                       'Programming Language :: Python :: 3.6',
                       'Programming Language :: Python :: 3.7',
                       'Programming Language :: Python :: 3.8'],
          packages=['hardshare'],
          install_requires=[
              'aiohttp',
              'cryptography',
              'pyjwt',
              'PyYAML'
          ],
          entry_points={'console_scripts': ['hardshare = hardshare.cli:main']}
          )
