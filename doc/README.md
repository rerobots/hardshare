Documentation
=============

The current release of this documentation is available at
https://docs.hardshare.dev

License and contributing guidelines are in the README file one directory up from here.

Current translation work is tracked at https://github.com/rerobots/hardshare/issues/1


Building
--------

A [Git LFS](https://git-lfs.github.com/) client is required to clone this
repository. Note that `git clone` will succeed without `git lfs` available, but
some large files will not be fetched.

Provided the dependencies (next section), you can get a local view of this
website via

    yarn dev

Now, direct your Web browser at http://127.0.0.1:3000/


Dependencies
------------

Dependency management is only tested with [Yarn](https://yarnpkg.com/). To
install what you need,

    yarn install

The most prominent dependencies to consider when you begin studying the source
code of this documentation are

* Markdoc, https://markdoc.dev/
* Next.js, https://nextjs.org/

The following fonts are included in the build and available from upstream under
the Open Font License:

* Orbitron, https://fonts.google.com/specimen/Orbitron/about
