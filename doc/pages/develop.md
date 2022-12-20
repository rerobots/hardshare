---
title: Development
---

# Development

## Building from Source Code

Clone the repository at <https://github.com/rerobots/hardshare> or download the
release source files.

To build,

```bash
cargo build --release --locked
```

Beware that the resulting program might be dynamically linked to libraries and,
therefore, not easily copied to a different host. For cross-compiling and
creating static programs (therefore avoiding linker dependencies at runtime),
releases are made with [cross](https://github.com/rust-embedded/cross).
For example, to build for Linux on Raspberry Pi,

```bash
cross build --target armv7-unknown-linux-musleabihf --release --locked
```