hardshare
=========

Introduction
------------

**hardshare** is a system for sharing your hardware through the
[rerobots](https://rerobots.net) infrastructure.

If you are a new or potential user, then begin reading at https://docs.rerobots.net/hardshare
where you will find instructions about installation and sharing your robots!

If you want to contribute to development, then read more below, and clone the
repository at https://github.com/rerobots/hardshare
Until version 1.0.0, the API between hardshare clients and rerobots servers
should be considered "strictly internal" and can change without warning.


Navigating the Sourcetree
-------------------------

The main repository is https://github.com/rerobots/hardshare.git

Besides the root README (you are reading it), the sourcetree contains more
README files in subdirectories that describe contents therein.

Summary:

* doc - source of the user guide.
* devices - code and configuration data for target hardware.
* src - main source code.


Building Documentation
----------------------

Go to doc/


Building and Testing
--------------------

This tool is implemented in [Rust](https://www.rust-lang.org/), and releases are
posted to the crate registry at <https://crates.io/crates/hardshare>. To build,

    cargo build

To perform tests,

    cargo test

To check code style,

    cargo +nightly fmt -- --check
    cargo clippy --tests -- -D clippy::all

To build for release on x86-64 Linux,

    cargo build --target x86_64-unknown-linux-musl --release --locked

Current [CI report](https://github.com/rerobots/hardshare/actions/workflows/main.yml):
![build status from GitHub Actions](https://github.com/rerobots/hardshare/actions/workflows/main.yml/badge.svg)


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
