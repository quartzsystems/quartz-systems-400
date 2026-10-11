# QS/400 (Quartz Systems/400)

A from-scratch operating system on the Linux kernel, written entirely in Rust,
source-compatible with IBM i and screen-identical at a 5250 terminal.
Phase 0 built the build system and a booting kernel with a Rust PID 1
(`slic`). Phase 1 made `slic` a real IPL with persistent state, system
values and a local 5250-style console. Phase 2 added the object store
(`store`), the disk layout with an ASP partition, system values in the
`QSYSVAL` object, RCLSTG, the service console's object commands and the
ISO installer. Addendum 2.5 added mirrored protection, Dedicated Service
Tools (`svctools`) and the IBM-style install flow. Code waiting for a
later phase is marked `MIGRATE(phaseN)` or `TODO(phaseN)`.

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
- Release identity is `V1R0M0` from the `release` crate; see ARCHITECTURE.md.
- Command line is baked: `console=ttyS0,115200 console=tty0 panic=-1 quiet`.
- Reproducible: fixed `SOURCE_DATE_EPOCH`/`KBUILD_BUILD_*`, deterministic cpio,
  deterministic GPT/FAT, signed content-hash manifest.

## Layout

```
build/        orchestrator: cargo run -p build -- <cmd>
  src/host/     cmd, paths, hash, doctor, deny, testrun
  src/kernel/   fetch, config, build (slot A and B UKIs)
  src/userland/ slic build + ELF check
  src/image/    cpio, initramfs, disk (via diskimg), iso (ISO 9660), media, manifest
kernel/       VERSION, SHA256, config/qs400.config
licpart/      LIC partition: A/B record (src/format/), locate by GPT type GUID
sysval/       catalog.toml, validation, src/tz/ time zones, LIC storage
console/      font/qs400-9x16.txt, src/fb/ framebuffer, src/keyboard/, screens
slic/         PID 1: src/ipl/ state machine, src/hwrsrc/, src/net/, src/svc/,
              src/platform/ (console, mounts, power, rtc, serial, sys), src/src.rs
conformance/  QEMU harness: src/qemu/ (session, qmp, serial, firmware), src/steps.rs
release/      VxRyMz identity, build ID, PTF names (one source of truth)
blockdev/     BlockDev trait, AlignedBuf, Linux O_DIRECT backend, MemBlockDev with faults
store/        object store: src/page/, src/btree/, src/asp/ (units, superblock),
              src/alloc/ (bitmaps, free tree), src/wal/, txn.rs, store.rs, cache.rs, src/obj/
diskimg/      GPT layout (ESP, LIC, ASP1), FAT volumes, partition lookup; build + installer
ccsid/        CP037 <-> UTF-8 from cp037.csv (build.rs), EBCDIC collation
qsname/       object, library and member name rules; 10-byte CP037 Name
svctools/     DST screens and the installer's: src/ui/ screen kit, panel.rs runner,
              src/dst/ (disk/, environment, tools, security), src/install/, src/users/
dds/          DDS: src/source/ records, src/lex/ columns and keywords, src/compile/
              field, key, select/omit and file-keyword builders, src/pf/ and src/lf/
              compilers, src/listing/
qdecimal/     Decimal (63 digits), IBM intermediate precision rules (src/arith/),
              packed, zoned, binary, integer and float encodings (src/encode/),
              edit codes and words (src/edit/), the IBM i oracle kit (src/fuzz/,
              src/bin/qdfuzz.rs; files under conformance/oracle/qdecimal/)
db/           database: src/file/ *FILE attributes, member and dependents tables, LF
              creation; src/format/ sections and level IDs; src/record/ field values;
              src/path/ key encoding and path trees; src/lf/ derivation and select/omit;
              src/member/ *MBR objects, record slots, path maintenance and rebuild;
              src/odp/ open data paths, RLA operations, record locks, INFDS feedback,
              OVRDBF overrides and the per-job session with shared paths;
              src/util/ CLRPFM, RGZPFM, CPYF (select, map), CPYTOIMPF/CPYFRMIMPF (impf);
              src/dsp/ DSPFD, DSPFFD, DSPPFM, DSPDBR; src/chg/ CHGPF, CHGLF, CHGPF SRCFILE;
              src/source/ source members (srcput, srcget, DDS source of a member);
              src/journal/ STRJRNPF/ENDJRNPF, the entries members and paths write,
              APYJRNCHG/RMVJRNCHG (undo.rs) and IPL recovery (recover.rs); src/cmt/
              commitment control (STRCMTCTL, COMMIT, ROLLBCK, lock levels)
jrn/          journaling: *JRN (src/journal/) and *JRNRCV (src/rcv/) objects, the entry
              layout (src/entry/), DSPJRN filters and the receiver chain cursor (src/dsp/)
sql/          SQL: src/lex/ tokens, src/ast/ statements (ddl, query, expr), src/parse/
              recursive descent (ddl, dml, query, expr); src/value/ runtime values;
              src/exec/ context and name resolution (mod.rs), expressions (eval.rs),
              scalar functions (func.rs), queries with path choice (query.rs), INSERT/
              UPDATE/DELETE/MERGE through the ODP (dml.rs), CREATE TABLE/INDEX/VIEW and
              labels as DDS (ddl.rs), cursors (cursor.rs), QSYS2 catalog views computed
              from the store (catalog.rs); src/sqlca.rs the SQLCA; src/run.rs RUNSQLSTM
docs/         ARCHITECTURE.md, PHASE0.md, PHASE1.md, PHASE2.md, STORE_FORMAT.md, DST.md,
              DB_FORMAT.md, DB_OPEN_QUESTIONS.md, DECIMAL.md, images/
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
cargo run -p build -- all         # the whole pipeline (dev: service console on)
cargo run -p build -- all --release       # no qs400.svc=1 on the cmdline
cargo run -p build -- iso         # UEFI-bootable installation media
cargo run -p build -- test        # the six-step Phase 1 suite under QEMU
cargo run -p build -- test --media iso    # the media steps
cargo run -p build -- test --media iso --install   # automatic install, boot, interactive reinstall
cargo run -p build -- deny        # cargo-deny + links/cc lockfile check
cargo clippy --workspace --all-targets    # must be warning-free
cargo clippy -p slic -p console --all-targets --target x86_64-unknown-linux-musl
QS400_QEMU=1 cargo test -p conformance    # the suite as a cargo test
cargo run -p qdecimal --bin qdfuzz -- kit conformance/oracle/qdecimal   # regenerate the oracle kit
```

Lint the Linux-only crates for the musl target too: their real code is
`cfg(target_os = "linux")` and the host build only sees stubs.

Service console (serial, dev builds): `help`, `srcs [table]`, `hist`, `hw`,
`lic`, `sysval NAME [VALUE]`, `mode manual|normal`, `screen`, `pwrdwn
[restart]`, `release`; store: `asp`, `lib ls`, `obj ls|crt|dlt|rnm|mov|dsp|bench`,
`dtaara get|set`, `dtaq snd|rcv`, `usrspc rd|wr`, `rclstg [*CHECK]`, `chkstg`,
`txn stats`, `unit add|rmv`, `mirror start|stop|suspend|resume|replace|status`,
`key NAME`, `type TEXT` (`QTEMP/NAME` addresses the temporary ASP). Boot parameters: `qs400.slot=A|B`, `qs400.mode=normal|manual`,
`qs400.svc=1`, `qs400.ipl=D`, `qs400.media=iso`.

`QS400_TARGET_DIR` relocates `target/` (use an ext4 path when the repo is on
`/mnt/c`; drvfs is far too slow for a kernel build). `-v` streams tool output;
otherwise logs go to `target/logs/`.

## Conventions

- Edition 2024, toolchain pinned in `rust-toolchain.toml`.
- Workspace lints: `unsafe_op_in_unsafe_fn = deny`, clippy `pedantic` warn,
  `missing_docs` warn. Clippy must stay clean.
- Every `unsafe` block has a `// SAFETY:` comment. `unsafe` lives only in
  `slic/src/platform/sys.rs` (signals), `slic/src/platform/rtc.rs` (one
  ioctl), `console/src/fb/raw.rs` (framebuffer ioctls),
  `slic/src/platform/blk.rs` (BLKRRPART), `blockdev/src/aligned.rs`
  (aligned buffer views) and `blockdev/src/linux.rs` (BLKDISCARD).
- Errors: `thiserror` enums in libraries, `anyhow` only in binaries.
- Logs: `tracing`.
- Small documented modules, one responsibility each. Private modules in
  binaries, `pub` only where a sibling needs it.
- Downloaded sources live under `target/src/` and are untrusted. Nothing is
  executed from there except the kernel's own `make` and `merge_config.sh`,
  always invoked with explicit paths.
- SRC codes live in one table, `slic/src/src.rs`; see docs/PHASE1.md.
- Every string a user sees is Quartz-written; never reproduce IBM message
  or help text. IBM names (system values, SRC families, screen geometry,
  DST menu titles and option numbers) are the compatibility interface.
- Every DST and installer screen has a golden file under
  `conformance/golden/`; `QS400_UPDATE_GOLDEN=1 cargo test -p svctools`
  rewrites them after a deliberate change.
- IPL state (slots, history, keylock, the IPL subset of the system
  values) stays in `licpart`; everything else persists in the object
  store.
- On-disk store structures are specified in `docs/STORE_FORMAT.md` before
  they are coded; `store/tests/spec.rs` compares the code's `Layout`s with
  the spec tables, so both change together. `docs/DB_FORMAT.md` and
  `db/tests/spec.rs` do the same for the database layouts.
- Framebuffer writes: one `pwrite` per frame spanning every line, then
  `fsync`. Partial-line writes lose damage at line ends and `mmap` never
  flushes under simpledrm's fbdev emulation (see docs/PHASE1.md).
