Development
===========

Building from Source Code
-------------------------

To build for own computer::

  cargo build --release --locked

Beware that the resulting program might be dynamically linked to libraries and,
therefore, not easily copied to a different host. For cross-compiling and
creating static programs (therefore avoiding linker dependencies at runtime),
releases are made with cross_.
For example, to build for Linux on Raspberry Pi, ::

  cross build --target armv7-unknown-linux-musleabihf --release --locked


.. _cross: https://github.com/rust-embedded/cross
