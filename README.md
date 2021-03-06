hardshare
=========

Introduction
------------

**hardshare** is a system for sharing your robot hardware through the
[rerobots](https://rerobots.net) infrastructure.

If you are a new or potential user, then begin reading at https://hardshare.readthedocs.io/
where you will find instructions about installation and sharing your robots!

If you want to contribute to development, then read more below, and clone the
repository at https://github.com/rerobots/hardshare

Until version 1.0.0, the API between hardshare clients and rerobots servers
should be considered "strictly internal" and can change without warning.

Because this project is new and because we have not yet identified the full set
of substantial use-cases in the wild, there are two protocols. The first is
based on an HTTP API and WebSockets, and the client is implemented in Python.
The second is already in development, but has not been announced yet.


Navigating the sourcetree
-------------------------

A [Git LFS](https://git-lfs.github.com/) client is required to clone this
repository. Note that `git clone` will succeed without `git lfs` available, but
some large files will not be fetched.

Besides the root README (you are reading it), the sourcetree contains more
README files in subdirectories that describe contents therein.

Summary:

* doc - source of the user guide. Instructions for building are below.
* robots - code and configuration data for particular robots.
* py-ws - source code for the main daemon and CLI program. Releases can be installed directly from PyPI at https://pypi.org/project/hardshare/
* bootstrapping - scripts for configuring new hosts on which to install hardshare clients.

Current testing status for ``master`` branch on Travis CI:
[![build status](https://travis-ci.org/rerobots/hardshare.svg?branch=master)](https://travis-ci.org/rerobots/hardshare)


Building documentation
----------------------

    cd doc
    pip install -r requirements.txt
    make

The `pip install` call is only required once to get required Python packages. If
you are working on translations to natural languages besides American English,
then

    make gettext-update

to generate and update gettext po files. Current translation work is tracked at
https://github.com/rerobots/hardshare/issues/1


Participating
-------------

All participation must follow our code of conduct, elaborated in the file
CODE_OF_CONDUCT.md in the same directory as this README.

### Reporting errors, requesting features

Please first check for prior reports that are similar or related in the issue
tracker at https://github.com/rerobots/hardshare/issues
If your observations are indeed new, please [open a new issue](
https://github.com/rerobots/hardshare/issues/new)

Reports of security flaws are given the highest priority and should be sent to
<security@rerobots.net>, optionally encrypted with the public key available at
https://rerobots.net/contact Please do so before opening a public issue to allow
us an opportunity to find a fix.

### Contributing changes or new code

Contributions are welcome! There is no formal declaration of code style. Just
try to follow the style and structure currently in the repository.

Contributors, who are not rerobots employees, must agree to the [Developer
Certificate of Origin](https://developercertificate.org/). Your agreement is
indicated explicitly in commits by adding a Signed-off-by line with your real
name. (This can be done automatically using `git commit --signoff`.)


License
-------

This is free software, released under the Apache License, Version 2.0.
You may obtain a copy of the License at https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
