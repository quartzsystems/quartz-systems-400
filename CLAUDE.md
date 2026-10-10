# QS/400 (Quartz Systems/400)

A from-scratch operating system on the Linux kernel, written entirely in Rust,
source-compatible with IBM i and screen-identical at a 5250 terminal.
Phase 0 is the build system and a booting kernel with a Rust PID 1 (`slic`).

## Ground rules

- Every source file starts with the license header (see `LICENSE`):
  ```
  // Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
  // licensed under the terms of the MIT license which can be found in the
  // root directory of this project.
  ```
- Comments are short and to the point.
- Claude never commits. At a natural checkpoint it prints a one-line commit
  message (under 60 chars) and waits for Cody to commit and say "continue".
- Pure Rust only. No crate may compile or link C (`cc`, `cmake`, `bindgen`,
  `links =`). `deny.toml` enforces this; `cargo run -p build -- deny` checks it.
- Source files are grouped into folders by concern (`host/`, `kernel/`,
  `image/`, ...), not left as one flat list under `src/`.

## Locked decisions

See `docs/ARCHITECTURE.md`. Short form:

- x86_64 is the only supported architecture. There is no second target and
  no arch abstraction in the code. The portable IR of a later phase exists
  for codegen upgrades, not for architecture migration.
- Linux kernel is the only inherited component. Userland is 100% Rust, static
  musl (`x86_64-unknown-linux-musl`).
- Kernel 6.18.y, LTS, built with `LLVM=1`, monolithic (`MODULES=n`), EFI stub,
  initramfs embedded. The bzImage is the UKI. Two ESP slots, A and B.
- Pin lives in `kernel/VERSION` and `kernel/SHA256`. Bumps are their own commit.
- Command line is baked: `console=ttyS0,115200 console=tty0 panic=-1 quiet`.
- Reproducible: fixed `SOURCE_DATE_EPOCH`/`KBUILD_BUILD_*`, deterministic cpio,
  deterministic GPT/FAT, signed content-hash manifest.

## Layout

```
build/        orchestrator: cargo run -p build -- <cmd>
  src/host/     cmd, paths, hash, doctor, deny, testrun
  src/kernel/   fetch, config, build
  src/userland/ slic build + ELF check
  src/image/    cpio, initramfs, disk, manifest
kernel/       VERSION, SHA256, config/qs400.config
slic/         PID 1 (src/platform/ holds console, mounts, sys)
conformance/  QEMU boot harness (lib + bin + test; src/qemu/ holds firmware, serial)
docs/         ARCHITECTURE.md, PHASE0.md
```

## Commands

Run from Linux (WSL Ubuntu on this machine). See `docs/PHASE0.md` for host setup.

```
cargo run -p build -- doctor      # host prerequisites table
cargo run -p build -- fetch       # kernel tarball, sha256, extract
cargo run -p build -- kernel
cargo run -p build -- userland
cargo run -p build -- initramfs
cargo run -p build -- uki
cargo run -p build -- disk
cargo run -p build -- manifest
cargo run -p build -- all         # the whole pipeline
cargo run -p build -- test        # boot under QEMU, check SRC codes
cargo run -p build -- deny        # cargo-deny + links/cc lockfile check
cargo clippy --workspace --all-targets    # must be warning-free
```

`QS400_TARGET_DIR` relocates `target/` (use an ext4 path when the repo is on
`/mnt/c`; drvfs is far too slow for a kernel build). `-v` streams tool output;
otherwise logs go to `target/logs/`.

## Conventions

- Edition 2024, toolchain pinned in `rust-toolchain.toml`.
- Workspace lints: `unsafe_op_in_unsafe_fn = deny`, clippy `pedantic` warn,
  `missing_docs` warn. Clippy must stay clean.
- Every `unsafe` block has a `// SAFETY:` comment. `slic` is `deny(unsafe_code)`
  except its `platform::sys` module.
- Errors: `thiserror` enums in libraries, `anyhow` only in binaries.
- Logs: `tracing`.
- Small documented modules, one responsibility each. Private modules in
  binaries, `pub` only where a sibling needs it.
- Downloaded sources live under `target/src/` and are untrusted. Nothing is
  executed from there except the kernel's own `make` and `merge_config.sh`,
  always invoked with explicit paths.
- SRC codes: `C1001000` IPL started, `C1001005` heartbeat, `B1001000` panic/fatal.
