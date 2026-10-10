# QS/400 architecture: locked decisions

These decisions are fixed. Do not revisit them without a new decision record.

## 1. One architecture: x86_64

QS/400 targets x86_64 only. This is a decision, not a deferral: there is no
second target in the toolchain, no target matrix, and no architecture enum or
dispatch anywhere in the code. The portable intermediate representation
planned for a later phase exists so code generation can be upgraded; it is
not an architecture migration path.

## 2. Inheritance

The Linux kernel is the only inherited component. Everything above the kernel
is written in Rust by Quartz Systems. The image contains no GNU software, no
BusyBox, no systemd, no glibc, no coreutils, no shell, and no prebuilt binary
of any kind.

## 3. Target and linking

- Rust target: `x86_64-unknown-linux-musl`.
- All userland is statically linked: `-C target-feature=+crt-static`, linked
  with the toolchain's own `rust-lld`, no host C toolchain involved.
- The orchestrator verifies every userland ELF: no `PT_INTERP`, no
  `DT_NEEDED`, no `.symtab`.

## 4. No C anywhere

No crate in the workspace may compile or link C code. A dependency is rejected
if it has a build-dependency on `cc`, `cmake` or `bindgen`, or declares a
`links =` key. `deny.toml` bans those crates (and the usual `*-sys` offenders);
`build deny` additionally scans `cargo metadata` for `links` keys and
`cc`-style build dependencies. The `libc` crate is permitted: it is bindings
to the system libc (musl, linked statically), not compiled C.

TLS for fetching sources uses `rustls` with the pure-Rust `graviola` provider.

## 5. Boot

- The kernel is built with the EFI stub and the initramfs embedded via
  `CONFIG_INITRAMFS_SOURCE`. The command line is baked with
  `CONFIG_CMDLINE_OVERRIDE`; the firmware-supplied one is ignored.
- The resulting `bzImage` is the unified kernel image. There is no GRUB and
  no systemd-boot stub.
- The ESP holds two slots: `EFI/BOOT/BOOTX64.EFI` is slot A,
  `EFI/QS400/B.EFI` is slot B. In Phase 0 both are the same image. The ESP
  contains nothing else.
- Command line: `console=ttyS0,115200 console=tty0 panic=-1 quiet`.

## 6. Kernel

- Linux 6.18.y (LTS). Only 6.18.y point releases, bumped by hand in
  `kernel/VERSION`. The pin is two committed files: `kernel/VERSION` and
  `kernel/SHA256`. A bump is its own commit.
- Source is `linux-<ver>.tar.xz` from `https://cdn.kernel.org/pub/linux/kernel/v6.x/`.
  On first run `build fetch` records the sha256 from the matching
  `sha256sums.asc` into `kernel/SHA256` and verifies the tarball. Afterwards it
  verifies against the committed file only and never re-fetches the sum. If
  `gpg` is present, the `sha256sums.asc` signature is checked against the
  kernel.org checksum autosigner key (fingerprint
  `B8868C80BA62A1FFFAF5FDA9632D3A06589DA6B1`, fetched over WKD) and the result
  is noted. No `gpg` is a warning.
- Built with `LLVM=1` (clang, lld, llvm-* binutils), monolithic,
  `CONFIG_MODULES=n`. Config = `x86_64_defconfig` + `kernel/config/qs400.config`,
  merged with the tree's own `scripts/kconfig/merge_config.sh`, then
  `olddefconfig`. The orchestrator parses the final `.config` and fails if any
  required symbol is not `=y` (or not `=n` for the banned set). It does not
  trust the merge.
- Initramfs compression is pinned to gzip (`RD_GZIP`, `INITRAMFS_COMPRESSION_GZIP`).
- `RANDOM_TRUST_CPU` no longer exists as a Kconfig symbol in 6.18 (removed in
  6.2). The kernel trusts the CPU RNG by default (`random.trust_cpu=on`),
  which is the intended behaviour, so nothing is set.

## 7. Init: `slic`

A Rust PID 1, static musl. Phase 0 scope:

1. Mount `proc`, `sysfs`, `devtmpfs`, `tmpfs` on `/run`. Already-mounted
   (`EBUSY`) is not an error.
2. Write `SRC C1001000` and the IPL banner to `/dev/console` and `/dev/ttyS0`.
3. Heartbeat `SRC C1001005` on serial every 5 s.
4. Panic hook writes `SRC B1001000` and the message to serial, then parks.
   PID 1 never exits and never reboots in Phase 0.
5. Reaps children on `SIGCHLD`.

`slic` is `deny(unsafe_code)` everywhere except a small `platform::sys`
module that holds the signal plumbing. Syscalls go through `rustix`; the
`libc` crate is used only inside `sys`.

## 8. Reproducible builds

- `SOURCE_DATE_EPOCH`, `KBUILD_BUILD_TIMESTAMP`, `KBUILD_BUILD_USER`,
  `KBUILD_BUILD_HOST`, `KBUILD_BUILD_VERSION` are fixed by the orchestrator.
- The initramfs cpio (newc) is written in Rust: sorted entries, mtime 0,
  uid/gid 0, sequential inode numbers, no host metadata.
- The disk image is written in Rust (`gpt`, `fatfs`): fixed disk and partition
  GUIDs, fixed FAT volume id and label, fixed (1980) FAT timestamps.
- `build manifest` writes a JSON manifest with sha256 and size of every
  artifact plus the inputs (kernel version and sha256, config hash, command
  line, toolchain identity, git revision) and signs it with ed25519. The
  signing key path comes from `QS400_SIGNING_KEY`; without it a dev key is
  generated under `target/keys/` and the build says so loudly.
- Two consecutive `build all` runs must produce byte-identical UKIs and
  manifests.

## 9. Build host

The orchestrator runs on Linux only (WSL Ubuntu is fine). Host prerequisites
are listed in `docs/PHASE0.md` and checked by `build doctor`. QEMU with OVMF
is test-only host firmware standing in for a machine's UEFI; it never ships
in the image.

## 10. Repository conventions

Cargo workspace, edition 2024, pinned toolchain, committed `Cargo.lock`.
Lints: `unsafe_op_in_unsafe_fn = deny`, clippy `pedantic` warn, `missing_docs`
warn; clippy must be warning-free. `thiserror` in libraries, `anyhow` in
binaries, `tracing` for logs. Small, documented modules grouped into folders
by concern. The repository will grow to 25+ crates; these conventions are set
now.
