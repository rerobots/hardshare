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
creating static programs, the correct
[target](https://doc.rust-lang.org/stable/rustc/platform-support.html)
must be specified.
A convenient tool to do this is [cross](https://github.com/cross-rs/cross).
For example, to build for Linux on Raspberry Pi,

```bash
cross build --target armv7-unknown-linux-musleabihf --release --locked
```
