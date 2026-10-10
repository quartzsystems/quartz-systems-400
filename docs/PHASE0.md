# Phase 0: toolchain and build system

What Phase 0 builds, how to run it, and the evidence for the acceptance
criteria. QS/400 is x86_64 only.

## What exists

- `build/`: the orchestrator. `cargo run -p build -- <cmd>`.
  - `fetch`: downloads `linux-<ver>.tar.xz` from kernel.org in Rust
    (`reqwest` over `rustls` + pure-Rust `graviola`), verifies the sha256
    against `kernel/SHA256`, extracts under `target/src/`. On first run it
    records the sum from `sha256sums.asc` (signature checked with `gpg`
    against the kernel.org autosigner when present) and writes `kernel/SHA256`.
  - `kernel`: `x86_64_defconfig` + `kernel/config/qs400.config` via
    `merge_config.sh`, `olddefconfig`, then `make LLVM=1 bzImage`. The final
    `.config` is parsed and checked symbol by symbol (61 symbols plus the
    command line).
  - `userland`: `cargo build --release -p slic --target x86_64-unknown-linux-musl`,
    linked with `rust-lld`. The ELF is parsed in Rust: no `PT_INTERP`, no
    `DT_NEEDED`, no `.symtab`.
  - `initramfs`: newc cpio written in Rust with `/init` (slic), `/dev`,
    `/dev/console` (c 5 1), `/proc`, `/sys`, `/run`. Sorted, zero mtimes,
    uid/gid 0, sequential inodes.
  - `uki`: rebuilds the kernel with `CONFIG_INITRAMFS_SOURCE` pointing at the
    cpio (gzip) and the baked command line. Output
    `target/images/qs400-<ver>-<hash>.efi`.
  - `disk`: GPT image with one 256 MiB FAT32 ESP holding
    `EFI/BOOT/BOOTX64.EFI` (slot A) and `EFI/QS400/B.EFI` (slot B), written
    with the `gpt` and `fatfs` crates. Fixed GUIDs, volume id and timestamps.
    The volume is re-read and must contain exactly those two files.
  - `manifest`: `target/images/manifest.json` with sha256 and size of every
    artifact and the inputs; `manifest.sig` is an ed25519 signature
    (`QS400_SIGNING_KEY`, else a generated dev key under `target/keys/`).
  - `all`: fetch, kernel, userland, initramfs, uki, disk, manifest.
  - `test`: boots the disk image under `qemu-system-x86_64` with OVMF and
    checks the SRC codes on serial.
  - `doctor`: host prerequisite table. `deny`: `cargo deny check` plus a scan
    of the compiled dependency graph (`cargo tree`, host and image target)
    for `links` keys and C build helpers.
- `slic/`: PID 1. Mounts proc, sysfs, devtmpfs, tmpfs on `/run`; prints
  `SRC C1001000` and `QS/400 IPL - phase 0` to `/dev/console` and
  `/dev/ttyS0`; heartbeats `SRC C1001005` on serial every 5 s; reaps on
  `SIGCHLD`; a panic prints `SRC B1001000` and parks.
- `conformance/`: QEMU driver (`-machine q35`, KVM when `/dev/kvm` is usable,
  else TCG, OVMF via `-drive if=pflash`). Asserts the IPL SRC within 60 s and
  two heartbeats within the next 15 s. Prints the serial log on failure.
  Also `cargo test -p conformance`, which runs the boot only when
  `QS400_QEMU=1`.

## Host prerequisites (Linux; WSL Ubuntu works)

Checked by `cargo run -p build -- doctor`.

| What | Why | Debian/Ubuntu package |
|---|---|---|
| rustup with `x86_64-unknown-linux-musl` | pinned by `rust-toolchain.toml` | `rustup target add x86_64-unknown-linux-musl` |
| clang, lld, llvm (LLVM 20) | `LLVM=1` kernel build; clang is also the linker driver for the host tools | `clang-20 lld-20 llvm-20` (and `clang lld llvm` meta packages) |
| make, flex, bison, bc, perl | kernel build | `make flex bison bc perl` |
| libelf headers | objtool | `libelf-dev` |
| openssl headers (optional) | only if kernel signing options are enabled | `libssl-dev` |
| tar, xz, gzip | source extraction, initramfs compression | `tar xz-utils gzip` |
| qemu-system-x86_64 | boot test | `qemu-system-x86` |
| OVMF | test-only UEFI firmware | `ovmf` (`/usr/share/OVMF/OVMF_CODE_4M.fd`) |
| git | manifest git revision | `git` |
| gpg (optional) | `sha256sums.asc` signature | `gnupg` |
| cargo-deny | acceptance check | `cargo install cargo-deny --locked` |

No GNU C toolchain is needed: `.cargo/config.toml` links the host tools
through `clang` and the image through `rust-lld`.

One-liner for Ubuntu:

```
sudo apt update && sudo apt install -y clang lld llvm make flex bison bc perl \
    libelf-dev libssl-dev xz-utils gzip qemu-system-x86 ovmf git gnupg
sudo usermod -aG kvm $USER     # then log out and in, for KVM
```

OVMF and QEMU are host test tools standing in for a real machine's UEFI. They
never ship in the image. Set `QS400_OVMF_CODE` if the firmware is elsewhere.

On WSL keep the build output on ext4: `export QS400_TARGET_DIR=$HOME/qs400-target`.
A kernel build on `/mnt/c` is many times slower.

## Running

```
cargo run -p build -- doctor
cargo run -p build -- all
cargo run -p build -- test
cargo run -p build -- deny
cargo clippy --workspace --all-targets
cargo clippy -p slic --target x86_64-unknown-linux-musl   # slic's Linux code
```

Environment: `QS400_TARGET_DIR`, `QS400_OVMF_CODE`, `QS400_QEMU` (binary
path), `QS400_NO_KVM=1`, `QS400_BOOT_TIMEOUT` (seconds), `QS400_SIGNING_KEY`,
`QS400_QEMU=1` for `cargo test -p conformance`. `-v` streams tool output;
logs are always in `target/logs/`.

## Acceptance evidence

Status at the x86_64-only scope change. The build host does not yet have
clang, QEMU or OVMF installed (no root), so the kernel-dependent items are
open. Everything else was exercised.

| # | Criterion | Status |
|---|---|---|
| 1 | `build all` succeeds on a clean checkout | open: needs the LLVM toolchain on the host |
| 2 | two runs give byte-identical UKIs and manifests | open: hashes recorded here once built |
| 3 | `build test` passes under KVM and under TCG (time documented) | open: needs QEMU + OVMF |
| 4 | `cargo deny check` passes; no `links`, no `cc` in the compiled graph | pass (see below) |
| 5 | `slic` has no `PT_INTERP`, no `DT_NEEDED` (checked in Rust) | pass (see below) |
| 6 | the ESP contains only the two `.efi` files | pass: enforced by `build disk`, confirmed by hand |
| 7 | `cargo clippy --workspace --all-targets` is warning-free | pass, also for slic on the musl target |
| 8 | CLAUDE.md, ARCHITECTURE.md, PHASE0.md written and accurate | this commit |

Verified so far (2026-10-10):

- `build userland`: slic is `static-pie linked, stripped`, 410688 bytes,
  sha256 `f8db97efcce3fc15837932cb5fbb1fce37a7949164ecbdf488d3ec1b3b57a188`.
  The ELF check in `build/src/userland/elf.rs` reports no `PT_INTERP`, no
  `DT_NEEDED`, no `.symtab`. `file(1)` agrees.
- slic run as a plain process refuses ("not PID 1") and exits 1. Run as PID 1
  in a user+pid namespace (`unshare -Urpf --mount-proc`) it mounts what it
  may, then idles in the heartbeat loop until killed.
- `build initramfs`: 411516 bytes. The newc headers were walked
  independently: `dev` 40755, `dev/console` 20600 rdev 5,1, `init` 100755,
  `proc`, `run`, `sys`, `TRAILER!!!`; every header 4-byte aligned, mtime 0,
  uid/gid 0.
- `build disk` with a stand-in UKI: `fdisk` sees a GPT with disk identifier
  `51533430-0000-4000-8000-000000000001` and one 256 MiB "EFI System"
  partition at LBA 2048; `file(1)` on the partition reports FAT32, serial
  `0x51533430`, label `QS400 ESP`. Only `EFI/BOOT/BOOTX64.EFI` and
  `EFI/QS400/B.EFI` exist on it.
- `build deny`: `cargo deny check` passes (advisories, bans, licenses,
  sources). The compiled graph is 143 crates on both the host and
  `x86_64-unknown-linux-musl`, with no `links` keys and no `cc`, `cmake`,
  `bindgen`, `pkg-config` or `vcpkg`. Note: `Cargo.lock` as a file still
  names `ring` (an optional dependency of `rustls-webpki` that nothing
  activates) and the wasm-bindgen crates (reqwest's wasm32-only
  dependencies). Neither is downloaded or built for any QS/400 target.
- `cargo clippy --workspace --all-targets` and
  `cargo clippy -p slic --target x86_64-unknown-linux-musl`: no warnings.
  `cargo test --workspace`: 5 tests pass (cpio layout, header fields,
  determinism, config parsing, and the boot test's skip path).

Results for items 1 to 3 are appended below once the host has the toolchain.
