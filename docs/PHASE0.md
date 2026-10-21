# Phase 0: toolchain and build system

What Phase 0 builds, how to run it, and the evidence for the acceptance
criteria. QS/400 is x86_64 only.

## What exists

- `tools/build/`: the orchestrator. `cargo run -p build -- <cmd>`.
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
- `lic/slic/`: PID 1. Mounts proc, sysfs, devtmpfs, tmpfs on `/run`; prints
  `SRC C1001000` and `QS/400 IPL - phase 0` to `/dev/console` and
  `/dev/ttyS0`; heartbeats `SRC C1001005` on serial every 5 s; reaps on
  `SIGCHLD`; a panic prints `SRC B1001000` and parks.
- `tools/conformance/`: QEMU driver (`-machine q35`, KVM when `/dev/kvm` is usable,
  else TCG, OVMF via `-drive if=pflash`). Asserts the IPL SRC within 60 s and
  two heartbeats within the next 15 s. Prints the serial log on failure.
  Also `cargo test -p conformance`, which runs the boot only when
  `QS400_QEMU=1`.

## Host prerequisites (Linux; WSL Ubuntu works)

Checked by `cargo run -p build -- doctor`.

| What | Why | Debian/Ubuntu package |
|---|---|---|
| rustup with `x86_64-unknown-linux-musl` | pinned by `rust-toolchain.toml` | `rustup target add x86_64-unknown-linux-musl` |
| clang, lld, llvm (LLVM 21) | `LLVM=1` kernel build; clang is also the linker driver for the host tools | `clang-21 lld-21 llvm-21` (and `clang lld llvm` meta packages) |
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

All measured on 2026-10-10 in WSL Ubuntu 26.04 (24 threads, KVM available),
Rust 1.98.0, Ubuntu clang 21.1.8, QEMU 10.2.1, OVMF 2025.11, at commit
`9aa28a9` plus this change.

| # | Criterion | Result |
|---|---|---|
| 1 | `build all` succeeds on a clean checkout | pass: 5 m 54 s wall from nothing (fetch, kernel, slic, cpio, UKI relink, disk, manifest) |
| 2 | two runs give byte-identical UKIs and manifests | pass: incremental second run and a clean third run (kernel build dir, images and slic deleted) match the first, hashes below |
| 3 | `build test` passes under KVM and under TCG | pass: KVM IPL SRC after 1.5 s, 11.6 s total; TCG IPL SRC after 3.8 s, 13.8 s total |
| 4 | `cargo deny check` passes; no `links`, no `cc` compiled | pass on Linux and Windows hosts; see note below |
| 5 | `slic` has no `PT_INTERP`, no `DT_NEEDED` (checked in Rust) | pass; `file(1)` says static-pie, stripped |
| 6 | the ESP contains only the two `.efi` files | pass; `build disk` re-reads the volume; boots run with `-snapshot` so the image stays pristine |
| 7 | `cargo clippy --workspace --all-targets` is warning-free | pass, also `cargo clippy -p slic --target x86_64-unknown-linux-musl` |
| 8 | CLAUDE.md, ARCHITECTURE.md, PHASE0.md written and accurate | this commit |

Artifact hashes, identical across run 1, run 2 (incremental) and run 3 (clean):

```
5a54bdaca67757233a2fc66109d1bfde265cd5a4dc439f9a60bf01388013f5c2  images/qs400-6.18.55-5a54bdaca677.efi  (14349312 bytes)
afa141794493d594e8b987acc7e12791098e86b38dc7be7512b6dd790a5d53fe  images/qs400-6.18.55-5a54bdaca677.img  (270532608 bytes)
1bfde7b3a5997fd4c7ab518c7b70c76982c4dc1718d33897e3b7eda54d697904  images/initramfs.cpio                  (411348 bytes)
cf7cea6e037d0f57b4b06e926ef2f71d6ef8ede5cf2b740335eb9980f1ad59df  x86_64-unknown-linux-musl/release/slic (410520 bytes)
7e16ca050750f00dc79f1724ca821496635a913afecc1985a465dc5a42a28e96  kernel/build/.config
43632d5504993b7c59b4011f9de9c57e92d25672e3575940baa57a49a5bb8523  images/manifest.json
07b9d2003ab7ab551681c7c979ad56420dd54b7df45cb84a22b18caab320d9a3  images/manifest.sig
```

Kernel pin: `linux-6.18.55.tar.xz`
`f410638061a165c12f42ab871d2f3fcd525515359b5faeee80969cff84524df9`,
recorded by the first `build fetch`; `sha256sums.asc` carried a good
signature from `autosigner@kernel.org`.

Notes:

- The manifest embeds `git_rev` (with `-dirty`), so it is identical between
  runs only at the same tree state. The UKI does not depend on git.
- OVMF, lacking a variable-store flash, writes an `NvVars` file to the first
  FAT volume it sees. The harness passes `-snapshot` so that write lands in a
  temporary overlay; the image hash was checked before and after both the
  KVM and TCG boots and did not change.
- `build deny` scans the graph that is compiled (`cargo tree`, host and
  image target): 143 crates, no `links` keys, no `cc`, `cmake`, `bindgen`,
  `pkg-config` or `vcpkg`. `Cargo.lock` as a file also names `ring` (an
  optional dependency of `rustls-webpki` that nothing activates) and the
  wasm-bindgen crates (reqwest's wasm32-only dependencies); neither is
  downloaded or built for any QS/400 target.
- Timing: `make bzImage -j24 LLVM=1` from a clean build directory is about
  4 minutes of wall time (60 CPU-minutes). The UKI relink after the cpio is
  embedded takes seconds.
- Unit tests: `cargo test --workspace` passes (cpio layout, header fields,
  determinism, config parsing). `QS400_QEMU=1 cargo test -p conformance`
  boots the image and passes in about 11 s.
