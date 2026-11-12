# buildkit: the build pipeline as a library

`tools/buildkit` is the build pipeline with the product taken out: the
pinned kernel, the static userland, the initramfs, the UKIs, the disk
image, the installation media and the signed manifest, for whatever
product a caller describes. `tools/build` is QS/400's orchestrator, a
thin binary over it (`tools/build/src/product.rs` is the description).
Another repository pins `buildkit` as a git dependency, writes its own
description and a `main.rs`, and has the same pipeline: the same kernel
pin layout, the same UKI and A/B ESP slots, with its own PID 1, kernel
fragment, initramfs and disk layout. QS/MC and QS/VS start that way.

The code: `tools/buildkit/src/` (`product.rs`, `ctx.rs`, `host/`,
`kernel/`, `userland/`, `image/`, `notices/`), `lic/diskimg/src/layout.rs`
(the general disk description), `lic/vrm` (release identity),
`lic/abrec` (the A/B record), `tools/buildkit/fixture/` (a second
product, built and booted by `build fixture`). Beside the pipeline, for
a product with an installer and a service console of its own (section
10): `lic/console`, `lic/scrkit`, `lic/svcusers`, `storage/protect`,
`lic/ublk` and `net/netlink`.

## 1. The product description

A caller makes one `buildkit::Product` and hands it to `Ctx::new`. The
fields:

| Field | What it is | QS/400's |
|---|---|---|
| `id` | lower-case identifier: artifact names `<id>-<release>-<kind>-<hash>.efi` and `<id>-<release>-<hash>.img`, `KBUILD_BUILD_USER` and `_HOST`, the extraction marker `.<id>-extracted`, the HTTP user agent `<id>-build` | `qs400` |
| `name` | the name as written, for the manifest's `product` | `QS/400` |
| `vendor` | the ESP vendor directory (`EFI/<vendor>/B.EFI`) and the ISO file's prefix | `QS400` |
| `env_prefix` | prefix of every environment variable the pipeline reads (section 4) | `QS400_` |
| `key_prefix` | prefix of the product's kernel command-line keys (`Product::key`) | `qs400.` |
| `repo` | the repository root, passed in by the caller; nothing in the library looks for it | two levels above `tools/build` |
| `release`, `build_id` | the release in artifact names and the manifest, and the build ID beside it | from the `release` crate |
| `fragments` | kernel configuration fragments under `kernel/config/`, merged in order after the baseline | `qs400.config` |
| `required_y`, `required_n` | symbols the final `.config` must and must not have `=y`, past the baseline's | the PASE namespaces, ublk, tape, the HBAs and NICs, ... |
| `cmdline` | the baked command line before each UKI's own words | `console=ttyS0,115200 console=tty0 panic=-1 quiet` |
| `packages` | the userland packages built for `x86_64-unknown-linux-musl`, each checked static and stripped; `manifest` says whether the signed manifest lists the binary | `slic`, `qsjob`, `instload` |
| `init` | the package that is `/init` in a plain initramfs (`image::initramfs::plain`) | `slic` |
| `kinds` | the UKIs to build: `SlotA`, `SlotB`, `Media` | all three |
| `uki_cmdline` | one UKI's command line; `dev` adds the development words | `qs400.slot=A`, ..., `qs400.svc=1` |
| `initramfs` | the entries of one UKI kind's initramfs, from the built binaries (`image::initramfs::binary`) | slic, qsjob and the trusted keys; the loader alone for the media |
| `disk_bytes`, `disk_parts`, `disk_guids` | the disk image: its default size, its partitions in order (`diskimg::layout::PartDesc`), fixed GUIDs | 4 GiB; `diskimg::Plan::parts` (ESP, LIC, ASP 1); `Guids::FIXED` |
| `esp_label`, `esp_volume_id` | the FAT volume label (eleven characters at most) and volume id of the ESP and of the media's boot image | `QS400 ESP`, `0x51533430` (`diskimg::fat`'s defaults) |
| `media` | what goes on the installation media around the media UKI (`image::iso::Media`), for a product that has media | the manifest, product loads, slot UKIs, slic and qsjob, the Knowledge Center, the PTF baseline |
| `doctor` | rows `doctor` adds to its own | the OVMF firmware |
| `deny` | graphs `deny` checks besides the workspace, each with its own advisory ignores | `tools/smbtest` |
| `notices` | the notices file's place in the tree and its opening lines | `lic/osobjects/shipped/NOTICES.txt` |
| `secure_boot_db` | the Secure Boot db certificate, whose SHA-256 the manifest records | `pesign`'s |

The hooks are plain function pointers; nothing is generic over the
product, and `Ctx` (the product, the paths, `verbose`, the `make` jobs)
is the one argument every step takes.

**The kernel baseline.** Every product's kernel gets a generated
fragment first (`kernel::config::baseline_fragment`): the EFI stub, the
baked command line with override, the embedded initramfs, xz, devtmpfs,
proc, sysfs and tmpfs on; modules and debug information off. The
product's fragments follow and have the last word; the final `.config`
is then checked symbol by symbol against the baseline's lists and the
product's.

**The disk.** `diskimg::layout` describes a disk as an ordered list of
partitions: a name, a GPT type, a size (`Fixed` bytes or `Rest` of the
disk, last) and what it starts with (`Keep`, `Image` for the ESP the
pipeline fills, `Wiped`, or an empty `Xfs`). `Layout::for_bytes` places
them (the first 1 MiB in, the last ending on a 4 KiB boundary before the
backup GPT) and `write_layout` writes the protective MBR, the GPT and
the contents. QS/400's `Plan` is one such description (`Plan::parts`),
and its writer is the general one; a unit test keeps the writer as it
was before and proves both write the same bytes
(`table::tests::the_general_writer_writes_the_same_disk`). The ESP holds
slot A at `EFI/BOOT/BOOTX64.EFI` and slot B at `EFI/<VENDOR>/B.EFI`.

**The media.** `image::iso` builds the UEFI El Torito ISO whose boot
image carries the media UKI, hybrid so it boots written raw to a disk,
around a tree the product supplies in order (`Item::File`, `Item::Link`
and one `Item::Boot`).

## 2. The steps

Each is a function over `&Ctx`:

| Step | Function |
|---|---|
| fetch | `kernel::fetch::fetch` |
| kernel | `kernel::kernel` |
| userland | `userland::userland` |
| initramfs | `image::initramfs::initramfs` |
| uki | `kernel::uki(ctx, dev)` |
| disk | `image::disk::disk(ctx, bytes)` |
| iso | `image::iso::iso` |
| manifest | `image::manifest::manifest` |
| doctor | `host::doctor::doctor` |
| deny | `host::deny::deny` |
| notices | `notices::notices(ctx, check)` |
| all of them in order | `buildkit::all(ctx, dev)` |

A consumer's `main.rs` is a clap enum and a `match` over these.

## 3. Starting a new repository

Taken from the fixture (`tools/buildkit/fixture/`), which is exactly
this:

```
VERSION                      0.3.0, the product's own release (V0R3M0)
rust-toolchain.toml          the pinned toolchain
kernel/VERSION               the kernel pin, as QS/400's
kernel/SHA256
kernel/config/qsdemo.config  only what the baseline lacks
init/                        the PID 1 package
build/                       the orchestrator package
Cargo.toml
```

`Cargo.toml`, with the QS/400 repository pinned by revision:

```toml
[workspace]
resolver = "3"
members = ["build", "init"]

[workspace.dependencies]
buildkit = { git = "https://github.com/quartzsystems/quartz-systems-400", rev = "<commit>" }
vrm = { git = "https://github.com/quartzsystems/quartz-systems-400", rev = "<commit>" }
diskimg = { git = "https://github.com/quartzsystems/quartz-systems-400", rev = "<commit>" }
abrec = { git = "https://github.com/quartzsystems/quartz-systems-400", rev = "<commit>" }

# The userland check wants the image's binaries stripped.
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = "symbols"
```

`build/build.rs` and `init/build.rs` capture the product's own release
and build ID:

```rust
fn main() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    vrm::build::capture(repo, "QSDEMO_");
}
```

`build/src/main.rs` is the description and the clap enum (the fixture's
in full: `tools/buildkit/fixture/build/src/main.rs`):

```rust
const RELEASE: vrm::Release = vrm::release_from_env!("QSDEMO_");

fn qsdemo() -> Result<Product> {
    Ok(Product {
        id: "qsdemo",
        name: "QSDEMO",
        vendor: "QSDEMO",
        env_prefix: "QSDEMO_",
        key_prefix: "qsdemo.",
        repo: /* this crate's parent */,
        release: RELEASE.to_string(),
        build_id: env!("QSDEMO_BUILD_ID").to_string(),
        fragments: &["qsdemo.config"],
        required_y: &["VIRTIO_BLK", "XFS_FS", "SERIAL_8250_CONSOLE"],
        required_n: &[],
        cmdline: "console=ttyS0,115200 panic=-1",
        packages: &[Package { name: "qsdemo-init", manifest: true }],
        init: "qsdemo-init",
        kinds: &[UkiKind::SlotA, UkiKind::SlotB],
        uki_cmdline: |p, kind, _dev| { /* cmdline + qsdemo.slot=A|B */ },
        initramfs: buildkit::image::initramfs::plain,
        disk_bytes: 1024 * MIB,
        disk_parts,   // ESP, an A/B record partition, one xfs data partition
        disk_guids: || LayoutGuids::fixed(*b"QSDM", 3),
        esp_label: "QSDEMO ESP",
        esp_volume_id: 0x5153_444D,
        media: None,
        doctor: |_| Vec::new(),
        deny: &[],
        notices: None,
        secure_boot_db: || None,
    })
}
```

Then `cargo run -p qsdemo-build -- all` fetches the kernel, builds it,
the userland, the initramfs, the two UKIs, the disk and the manifest
under `target/`.

The PID 1 keeps its state in an A/B record of its own (`init/src/main.rs`
in full in the fixture): an `abrec::Spec` with the product's magic and
the GPT type of the partition the disk description gives it, and a
payload that is the product's business (QSDEMO's is a boot count).

```rust
const RECORD: abrec::Spec = abrec::Spec {
    magic: *b"QSDEMOAB",
    version: 1,
    type_guid: uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d01"),
    slot_bytes: 4096,
    min_payload: 4,
};

let dev = OpenOptions::new().read(true).write(true).open(abrec::locate(&RECORD)?)?;
let mut part = abrec::AbPart::open(RECORD, dev);
let boots = match part.read() {
    Ok(copy) => u32::from_le_bytes(copy.payload[..4].try_into()?),
    Err(abrec::AbError::NoValidCopy) => 0,
    Err(e) => return Err(e),
};
let (slot, sequence) = part.write(&(boots + 1).to_le_bytes())?;
```

`cargo run -p build -- fixture` (in this repository) proves it: it
copies the fixture to a clean directory under the target directory,
writes its `Cargo.toml` with `git = "file://<this repository>"` at HEAD
(so the git-dependency form is what is built, and only committed work is
seen), copies the kernel and toolchain pins and the kernel tarball this
build already fetched, runs the fixture's own `all`, boots a copy of its
disk image under QEMU with OVMF, and passes when the serial console
shows `QSDEMO V0R3M0 fixture` (the fixture's release, which must differ
from QS/400's) and QEMU exits cleanly after the power off. It also
asserts the two things a disk carries a product's name in: the ESP's
FAT label and volume id are the fixture's (`QSDEMO ESP`, not `QS400
ESP`), and the A/B record its PID 1 wrote is on the partition of the
fixture's GPT type under the magic `QSDEMOAB`, sequence 1 in copy A,
which `licpart` does not read as a LIC record. `build test` (the disk
suite) runs it as its last step.

The fixture's PID 1 then does what an appliance's installer and service
console do with the crates of section 10, and the step asserts each on
its console (`fixture/init/src/panel.rs`, `raid.rs`, `net.rs`):

- **A panel.** A menu built with `scrkit` under the identity `QSDEMO
  V0R3M0 fixture`, system `QSDEMO1`, shown by `console` on the
  framebuffer (the step gives QEMU a display adapter; the fixture reads
  `/dev/fb0` back and counts what is lit) and on the serial console in
  ANSI sequences (the step finds the title and `System:  QSDEMO1` drawn
  there, and no `QS/400` anywhere). The bytes a terminal would send
  (`9` Enter, `2` Enter, F5, `1` Enter) go through `KeyDecoder` and end
  the panel with its outcome.
- **A protected set.** Two 96 MiB image files on the data partition,
  mirrored by `protect::set::MirrorSet`, served as `/dev/ublkb0` by
  `ublk` from a process of its own, formatted by `xfsfmt`, mounted and
  written; member 1 detached and its file deleted, the data read back
  from the device (caches dropped); the server killed, a read left
  waiting, a new server recovering the device under the mounted xfs,
  the read answered and more written; a new file put in member 1's
  place and resumed; the two members compared byte for byte.
- **A route.** `198.51.100.0/24` on `lo` added through `netlink` with
  routing protocol 83 and read back from the kernel with it.

And `cargo tree` of the fixture's PID 1 names `console`, `scrkit`,
`protect`, `ublk` and `netlink`, and none of `release`, `licpart`,
`store`, `svctools`, `tn5250` or `ccsid`.

## 4. Environment variables

The pipeline's own variables carry the product's prefix:

| Variable | Use |
|---|---|
| `<PREFIX>TARGET_DIR` | the output root; else `CARGO_TARGET_DIR`, else `<repo>/target` |
| `<PREFIX>JOBS` | caps the kernel's `make -j` |
| `<PREFIX>SIGNING_KEY` | the ed25519 key the manifest is signed with; else a development key under `target/keys/` |
| `<PREFIX>BUILD_ID`, `<PREFIX>RELEASE` | read by `vrm::build::capture` in the product's `build.rs`: the build ID instead of `git describe`, a release other than `VERSION`'s |

`CARGO`, `RUSTC` and `PATH` are read as cargo sets them. QS/400's other
variables are QS/400's and stay in `tools/build` and the crates that
read them: `QS400_TRUSTED_KEYS` (the initramfs), `QS400_INSTALL_RSP`
(the media), `QS400_OVMF_CODE` and `QS400_OVMF_VARS` (`conformance`),
`QS400_SB_KEY` and `QS400_SB_CERT` (`pesign`).

## 5. What stays in `tools/build`

- `main.rs`, the clap surface, and `product.rs`.
- `image/media.rs`: what QS/400 puts on its media (the signed media
  manifest through `instmedia`, the product loads, slic and the job
  program for the media UKI's loader, the response file, the Knowledge
  Center and PTF baseline trees, the db key in the boot image).
- `image/lp.rs`, `ptf.rs`, `release.rs`, `srcoffer.rs`: product loads,
  PTFs, the release directory and the kernel source offer.
- `host/testrun.rs`, `soak.rs`, `bench.rs`, `fuzz.rs`, `fixture.rs`: the
  QEMU suites and what runs beside them.
- `licenses/linux-COPYING.txt`: the source offer's copy of the kernel's
  license. The license texts the notices use moved with them to
  `tools/buildkit/licenses/`.

Notes on the edges:

- `buildkit` depends on `optical` for the ISO 9660 writer, besides
  `diskimg`, `blockdev` and `xfsfmt`. `optical` depends on `thiserror`
  only and is an accepted dependency of the library (handoff 13a): the
  ISO step cannot be in it otherwise. The neutrality test's allowed
  list is these four.
- `buildkit` does not depend on `pesign` (nor so on `certs`): the
  manifest's Secure Boot field comes through `Product::secure_boot_db`.
- The hybrid image's second partition has `optical`'s Quartz media type
  for every product.
- A FAT volume's label and volume id are part of `diskimg::fat::Spec`.
  The specs `diskimg` hands out (`DISK_ESP`, `MEDIA_ESP`, `esp_spec`,
  `media_esp`, `fat32_for`) carry QS/400's, `QS400 ESP` and `QS40`,
  which is what `slic`'s installer writes; the pipeline puts the
  product's over them (`Product::fat_spec`).
- The fetch step keeps QS/400's kernel line: a `6.18.y` pin from
  kernel.org's `v6.x` directory.

**Beside the pipeline.** What an appliance's installer and service
console are made of is not in `tools/build` either, and no longer in a
QS/400-shaped crate (handoff 14). Each of these is neutral and takes
what names its owner from the caller; section 10 has the detail.

| Crate | What it is | From its caller |
|---|---|---|
| `console` | the 24x80 screen with 5250 attributes, the field editor, the font, evdev keyboards, the SRC display and application screens, on the framebuffer and on a serial terminal | its `Identity` (the banner: product, release, build; the code after which a sign-on may show), the system name, the command-line key of the scale |
| `scrkit` | the screen builders and the panel stack | every title, the system name, the function key line, the messages; the environment its panels ask and the outcome they end with |
| `svcusers` | service tools user IDs: salted hashes, privileges, lockout | the users it ships with, and where they are kept (`UserStore`) |
| `protect` | mirrored and parity protection, resynchronization, hot spares, over `BlockDev` units; a set as one `BlockDev` | what identifies a member and records its state; the resume map's keeper; the on-disk constants of its sets |
| `ublk` | a kernel block device served by a process | the device behind it (a `Target`, or any `BlockDev`); whether it outlives its server |
| `netlink` | rtnetlink, sock_diag, nftables, sysctl | the routing protocol number of its routes, the name of its nftables table |

## 6. Crates another repository takes

`build fixture` builds all fifteen as git dependencies, and its PID 1
runs `console`, `scrkit`, `protect`, `ublk` and `netlink`. What an
appliance should know of each:

| Crate | As it is |
|---|---|
| `buildkit` | product-neutral (a test fails on a product's name in its code) |
| `vrm` | product-neutral: `Release`, `build::capture`, `release_from_env!` |
| `diskimg` | the general layout is neutral; `Plan`, its GUIDs and partition names are QS/400's; `fat::Spec`'s default label and volume id are QS/400's, set your own (the pipeline does) |
| `blockdev` | neutral |
| `xfsfmt` | neutral; the default label is `QS400-IFS`, set `Options::label` (the layout writer does) |
| `ipc` | neutral: the wire codec, frames, `rpc!`, Unix socket channels |
| `abrec` | product-neutral: the two-copy A/B record with its header, sequence, CRC and slot selection (`AbPart`, `encode`/`decode`, `newest`/`target`). The magic, the format version, the partition's GPT type and the slot size come from the caller's `Spec`; the payload is opaque bytes. The fixture's PID 1 uses it with its own |
| `licpart` | QS/400's record over `abrec`: the magic `QS400LIC`, the LIC partition's type GUID, the IPL state in the payload (slots, keylock, the IPL system values, the install answers) and the resume record `QS400RSJ` beside the copies. An appliance takes `abrec`, not this |
| `console` | product-neutral (section 10). With `default-features = false` it is the screen, the editor, the font, the keyboards, the SRC display and the two displays, and depends on nothing else of this repository; the default feature `session` adds the sign-on display and the 5250 terminal, with `tn5250` and `ccsid` |
| `scrkit` | product-neutral: `ui` and `panel` over `console` |
| `svcusers` | product-neutral: `UserDb`, `Privilege`, `UserStore` and `FileStore`; `ipc` with the feature `wire` |
| `protect` | product-neutral: over `blockdev` only. It writes no label of its own (section 10) |
| `ublk` | product-neutral: `Device`, `Target`, `BlockTarget`, `Options` |
| `netlink` | product-neutral: nothing of this repository under it |
| `svctools` | QS/400's: the DST and installer screens, over `scrkit`, `svcusers`, `console`, `licpart`, `release` and `hwqual`. An appliance takes the three neutral ones, not this |

They build from outside because cargo resolves a git dependency's
`workspace = true` keys against the workspace in the checkout. `release`
itself is not reusable: its `build.rs` reads this repository's `VERSION`
and `QS400_*` variables, which is why `vrm` exists.

## 7. Baseline

Section 0 of the handoff, at `6296b9ab8` (the tree had moved from
`cb121aa9`: the media UKI's loader and QEMU step 171). `build all` twice
with `QS400_BUILD_ID=buildkit-baseline` (the build ID is otherwise the
commit, which every later commit changes); both runs agree.

| Artifact | SHA-256 before |
|---|---|
| `initramfs.cpio` | `5da055cd9cb98250ab6554355452a85c5815a6e48501994f4d2dbb3b78d72fbc` |
| `initramfs-media.cpio` | `ebe0366223c81c08c088cb9c405cac2eb3da83c1bdb77aa8b545ca8182fe68cc` |
| `uki-a` | `ee58658aca3be6181c177999cbab08d95fec10a707306371a81f3492ce1b1b47` |
| `uki-b` | `b683acafb24c2c73368d91cfb67436fb09e4c94230272e1bccb031b21db74e84` |
| `uki-d` | `d3b4927607259c85391574dafc279e85dd48534dcdf5d055d4aa10ce4e7cb448` |
| disk image | `5e46f7f996c2dc0966ee18cf590af431ffa59bbdeef975b34246b20083b728da` |
| ISO | `2cd1b6684ec965ea544873afa148838928eee588a21d71ca873984f2061a04ab` |
| `manifest.json`, `git_rev` masked | `2a903b19ea23d8e134ba4e6f6640e05dd5c28ae32d3ea7b5364964c5cda98f6d` |

After, in two parts, because the handoff changes two kinds of thing:

1. **The pipeline moved into the library (sections 1 and 2 of the
   handoff, `976c9229e`).** Nothing the image links changed, and `build
   all` gives every hash above again. The manifest's content differs in
   one field, `cargo_lock_sha256`, since `Cargo.lock` gained the new
   crate.
2. **`diskimg` and `release` restructured (sections 3 and 4).** `slic`
   and `qsjob` link both, so their bytes change (same code, moved
   between crates and functions) and with them the slot initramfs, the
   slot UKIs, the disk and the ISO; the media initramfs and `uki-d` do
   not change. What the pipeline does with them is proven unchanged
   another way: the userland binaries are built from the baseline commit
   and the final tree's `initramfs`, `uki`, `disk`, `iso` and `manifest`
   steps run over them.

| Artifact | Before | After the library split | Final tree over the baseline's userland |
|---|---|---|---|
| `initramfs.cpio` | `5da055cd9cb9` | same | same |
| `initramfs-media.cpio` | `ebe0366223c8` | same | same |
| `uki-a` | `ee58658aca3b` | same | same |
| `uki-b` | `b683acafb24c` | same | same |
| `uki-d` | `d3b492760725` | same | same |
| disk image | `5e46f7f996c2` | same | same |
| ISO | `2cd1b6684ec9` | same | same |
| `manifest.json`, `git_rev` masked | `2a903b19ea23` | differs in `cargo_lock_sha256` only | differs in `cargo_lock_sha256` only |

"Same" is the full SHA-256 of the table above, compared by `diff` of
the sums. The baseline's `slic` and `qsjob` were rebuilt from
`6296b9ab8` at the same checkout path (the binaries carry the path, so
a worktree elsewhere does not reproduce them) and matched the hashes the
baseline's manifest recorded (`9da4d5848808`, `fcca815edd23`) before the
final tree's steps ran over them.

With the final tree's own userland the slot artifacts are other bytes
(`slic` links the restructured `diskimg` and `release`). What that
leaves to prove is behaviour, not layout: `diskimg`'s old writer is kept
in its tests and compared byte for byte with the general one; `vrm` and
`release` keep their tests; the QEMU steps below ran on the final tree.

## 8. Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Section 0 hashes identical before and after | identical after the split; identical for the final tree over the same userland binaries; the final userland itself differs because sections 3 and 4 change crates it links (section 7) |
| 2 | Every existing `build` subcommand and flag works as before; `build test` passes | the clap surface is unchanged but for the added `fixture`; `build all`, ran locally on the final tree: the disk boot, sign-on and system values, the five media steps (the media IPL of an installed disk among them), the automatic and the interactive install, both through the general disk writer in `slic`; the whole disk suite is CI's |
| 3 | `buildkit` dependency test | `cargo test -p buildkit --test neutral`: no operating-system crate in `cargo tree -p buildkit`; its crates of this repository are `diskimg`, `blockdev`, `xfsfmt`, `optical` |
| 4 | `build fixture` boots to the `QSDEMO` banner with its own release | `QSDEMO V0R3M0 fixture`, powered off |
| 5 | The ten crates compile from the fixture as git dependencies | `buildkit`, `vrm`, `diskimg`, `abrec`, `blockdev`, `xfsfmt`, `licpart`, `ipc`, `console`, `svctools`, by `git = "file://..."` at HEAD |
| 6 | No `QS400`/`qs400` literal in `tools/buildkit/src` outside tests and docs | `cargo test -p buildkit --test neutral` (`names_no_product`) |

`cargo deny`, `build notices --check` and `tools/fmtcheck` pass with no
version bump.

## 9. Handoff 13a

What handoff 13 left open, closed:

| # | Check | Result |
|---|---|---|
| 1 | `optical` is an allowed dependency of `buildkit` | in the neutrality test's list and section 5 |
| 2 | A neutral A/B record, `licpart` rebuilt on it, the bytes on disk identical | `lic/abrec`; `lic/licpart/tests/bytes.rs` compares a device written through `LicPart` (three record writes, the resume area) with `tests/golden/`, which the code before the change wrote at `bf2b492c5`, byte for byte, and reads that golden back; `fmtcheck` has the `licpart` format at version 1 with `abrec`'s two format files added to it, its hash re-locked (`update --force`, the one format that moved) |
| 3 | The FAT label and volume id are the caller's | `diskimg::fat::Spec::label` and `volume_id`; `Product::esp_label` and `esp_volume_id`; QS/400 keeps `QS400 ESP` and `0x51533430` (`fat::tests::the_defaults_are_what_every_volume_had`, `product::tests::the_fat_volumes_keep_their_name`) |
| 4 | The fixture uses `abrec` with its own magic and GUID and its own ESP label | `build fixture` asserts both on the booted disk (section 3): `QSDEMO V0R3M0 fixture`, ESP `QSDEMO ESP`, record `QSDEMOAB` |
| 5 | The section 7 baseline again | at `d3ba4fc20`, the tree's `initramfs`, `uki`, `disk`, `iso` and `manifest` steps over the baseline's userland binaries (`slic` `9da4d5848808`, `qsjob` `fcca815edd23`, `instload` `49872df9deea`), `QS400_BUILD_ID=buildkit-baseline`: the seven artifact hashes of section 7 by `diff` of the sums; `manifest.json` differs in `cargo_lock_sha256` only (`Cargo.lock` gained `abrec`) |
| 6 | The tree's own userland still boots, installs and switches slots | as in section 7, `slic` links `licpart`, `abrec` and `diskimg`, so its bytes are other bytes; QEMU on the tree's own `build all`: the IPL, telnet sign-on (51), the LIC PTF to slot B and back (164), the automatic install, the boot of the installed disk and the interactive install |

`console` and `svctools` stay QS/400's, as section 6 said then; handoff
14 (section 10) took `console` out of that and split `svctools`.

## 10. Handoff 14: the screen kit, disk protection, netlink

QS/VS and QS/MC get an installer and a service tools console with
software RAID on their load-source disks. Four things they need were
inside QS/400 crates in a QS/400-specific form; each is now a neutral
crate and QS/400's use of it. Nothing QS/400 shows or writes changed:
the screens are the same bytes, and so are the units of an ASP.

### What each crate takes from its caller

**`console`.** `Console::open*` takes an `Identity`: the banner on the
SRC display's first line (QS/400 passes `release::banner()`) and the
reference code after which a sign-on display may show (`C6001090`). The
system name comes with each `ShowSrc`, as before. `Scale::from_cmdline`
takes the command-line key (`qs400.console.scale` is slic's). The crate
no longer depends on `release`. QS/400's own screens stay behind a
feature: `session`, on by default, is the sign-on display, the console
as the terminal of a 5250 session, and the `tn5250` and `ccsid`
dependencies; with `default-features = false` none of that is compiled.
The font is the one `.txt` file of `font/` (it keeps its name, which
Access for Web serves and the client repositories carry).

**`scrkit`.** `ui` is the screen builders `svctools` had (`header`,
`menu`, `menu_after`, `menu_at`, `list`, `form`, `form_wide`, `text`,
`confirm`, `message`): the fixed words (`Selection`, `Select one of the
following:`, `System:`, `F10=Continue   F12=Cancel`) are Quartz's and
name no product; titles, the system name, the function key line and the
messages are arguments. `panel` is the stack: `Stack<P>` and
`Flow<P, O>` over any panel type and outcome, and a ready
`Panel<E, O>` and `Runner<E, O>` for a caller whose panels ask one
environment type `E`. `svctools` keeps its own `Panel` trait over the
same `Stack` and `Flow`, because its environment is a borrowed trait
object (`&mut dyn DstEnv`, which `slic` implements over borrowed state);
its `Outcome`, `dst/`, `install/`, `env.rs` and SST's menu layout stay
with it.

**`svcusers`.** `UserDb` (create, delete, sign on, change and reset a
password, enable, privileges, the last-administrator rule), Argon2id
hashes with the caller's salt, three wrong passwords disable an ID. The
caller makes the users it ships (`UserDb::push`) and keeps the bytes of
`UserDb::encode` through a `UserStore`; `FileStore` is one in a file
(written beside, flushed, renamed). `svctools::users` is now QS/400's
part only: `LicUsers::defaults` (`QSECOFR`, `QSRV`, `11111111`,
`22222222`), `reset_secofr`, and `load`/`store` in TLV `0x0102` of the
LIC record. The bytes are the same.

**`protect`.** Over `blockdev` and nothing else of this repository:

- `UnitState`, with the bytes a unit table records.
- `mirror`: `Halves` is the I/O over the members of a mirrored set lent
  for one transfer (read from one, alternating; write and flush to every
  member that takes writes; a failing member noted for the caller to
  take out of service; a block that fails the caller's check on one
  member read from the next and rewritten). `copy_step` is the
  resynchronization, a megabyte chunk at a time, over a `ResumeIo` the
  caller implements (read the member in step, whether a block is wanted,
  write, flush, save the map); `ResumeState` is the map of chunks done,
  encoded under a magic of the caller's.
- `parity`: the RAID 5 and RAID 6 engine with its partial-parity log
  (`Engine`, `Geometry`, `gf`, `ppl`, `ParityDev`, `SetHandle`). A
  caller's `Format` gives the two magics its slots and state page carry
  and its test for a damaged page; `Geometry::new` takes the pages at
  the head of each member the set leaves alone.
- `spare::smallest_fit` and `spare::take`: which hot spare takes a
  failed member's place.
- `set`: a protected set as one `BlockDev`, for a caller with no pool:
  `MirrorSet` (two or more members; suspend, detach, replace, resume
  with a `ResumeJournal`, hot spare takeover) and `ParitySet` (the
  members' rows end to end; synchronization and rebuild in steps,
  detach, replace, hot spare takeover, verify), and `Window`, a run of
  a device's blocks as a device.

`store` is built on it: `asp/io.rs` lends each span's units to
`Halves`, `asp/mirror.rs` runs `copy_step` (its `ResumeIo` copies every
allocated page and none of the reserved ones), `asp/parity/` keeps the
table and the operations and passes `FORMAT` (`QS400PPL`, `QS400PST`,
the page trailer's checksum) and a head of three pages, `asp/spare.rs`
asks `smallest_fit`. Its API is as it was (`slic` did not change for
it).

**`ublk`.** `Device::start` over any `Target`; `BlockTarget` makes any
`BlockDev` one. `Device::start_with(target, Options { recovery: true })`
adds the device so that it outlives its server, and
`Device::recover(id, target)` is the next server taking it over.

**`netlink`.** `Route::new(family, dst, len, protocol)` and
`Nft::open(table)` (and the `nft` builders) take what was
`RTPROT_QS400` and the table `qs400`; QS/400's values are
`tcpcfg::net::kernel::RTPROT_QS400` (81) and `NFT_TABLE`, and what it
puts in the kernel is the same.

### The reports

**Can `console` drive a serial terminal as well as the framebuffer?**
It could not. Its one renderer was the framebuffer's; `slic`'s serial
console mode only kept the screen in memory for the service console's
`screen` command. Now there is a trait, `console::Display` (`draw`,
`describe`, `check_mode`), which the framebuffer's renderer implements,
and a second implementation, `console::ansi::AnsiDisplay`, that draws
the grid on anything that takes bytes with ordinary ANSI sequences:
cursor addressing, SGR colors (the 5250 colors on the sixteen ANSI
ones, high intensity as the bright ones), underline for input fields,
reverse image and blink; nondisplay fields show nothing, and only the
cells that changed are sent. `Console::add_display` shows every screen
on it as well. `console::ansi::KeyDecoder` turns the terminal's bytes
into the console's keys, which go in as `Command::Virtual`:

| Key | From the terminal |
|---|---|
| Enter, Field Advance, Field Backspace, Backspace | CR or LF, Tab, `CSI Z`, BS or DEL |
| cursor keys, Home, Insert, Delete, Page Up, Page Down | `CSI A`-`D` (and `SS3`), `CSI H` or `1~`, `2~`, `3~`, `5~`, `6~` |
| F1 to F12 | `SS3 P`-`S`, `CSI 11~` to `24~`; or Esc and `1`-`9`, `0`, `-`, `=` |
| F13 to F24 | the same with Shift (`CSI 1;2P`, `CSI 15;2~`, `CSI 25~` to `34~`); or Esc and `!@#$%^&*()_+` |
| Field Exit, Reset, Attention, System Request | Ctrl+K, Ctrl+R, Ctrl+A (or Esc alone), Ctrl+T |

QS/400 attaches no serial display: its serial line stays the service
console.

**Are the service tools user IDs separable from `licpart`?** Yes. Of
`svctools::users`, two functions touched the LIC record (`load` and
`store`, one TLV); the rest never did. The neutral part is `svcusers`,
with the storage behind `UserStore`, so an appliance keeps its users in
a state file (`FileStore`) or in a record of its own. What stayed in
`svctools` is above.

**Can the protection engine leave the store without a format change?**
All of the engine did, and no byte changed. What did not move is where
the store records a set, because that is store format:

- the unit header (page 0 and the last page of each unit: unit and ASP
  UUIDs, the unit number, the mirror partner), a store page sealed with
  the store's page trailer;
- the unit table and the parity table in the superblock (each unit's
  state, partner and usable pages; each member's set, position, level
  and state; the work under way), in the superblock's two slots.

So `protect` writes no label of its own, and a set is opened from a
record its caller keeps. What an appliance has to supply, exactly:

1. **Which devices make the set, in member order**, and the set's size
   in blocks (for a mirrored set) or its `Geometry` (for a parity set:
   level, member count, blocks, head; made once by `ParitySet::plan`
   and kept, whatever the size of a disk replaced since).
2. **Each member's identity and state**, read from `members()` and
   written to its own state whenever the set reports an event
   (`drain_events`) and after `suspend`, `detach`, `replace`,
   `take_spare` and the end of a resume or rebuild: `UnitState` for a
   mirrored member, `Health` (with the rebuild mark) and the
   synchronization mark for a parity set. A mirrored set has nothing on
   its members that says one is stale: the record must be durable
   before the appliance goes on after a member failed. A parity set
   also records a failed member on the others' state pages, so a
   record that is behind is corrected at open.
3. **The resume map**, through `set::ResumeJournal` (`load`, `save`,
   `clear`), encoded with a magic of its own.
4. **For a parity set, a `Format`**: two eight-byte magics of its own
   and its test for a damaged page (`|_| false` without checksums).
5. **How it knows a disk**: by serial number or partition GUID from its
   own record, or by a label of its own in blocks it keeps out of the
   set with a `Window` (or a `Geometry` head).

The fixture does this with a record in memory in its PID 1 (the server
process is told the members and their states when it starts) and a
resume map in a file under its own magic.

`NEEDS-CODY`: whether the appliances should share an on-disk member
label (a second metadata format beside the store's unit header and
superblock, written by `protect` on each member) rather than each
keeping the record above in its own state. None was defined here:
nothing in this handoff needs one, and it is a format to freeze.

**Does `ublk` serve any `BlockDev`, and survive its server's restart?**
It served any `Target` already (the IFS volume is one); `BlockTarget`
is the adapter for a `BlockDev`. It did not survive a restart: the
device went with its server, which is what QS/400 wants (the storage
service ending is the system ending). `Options::recovery` adds the
device with `UBLK_F_USER_RECOVERY` and `UBLK_F_USER_RECOVERY_REISSUE`:
when the server ends without `stop`, the kernel keeps `/dev/ublkbN`,
holds new requests and requeues the ones the server had taken;
`Device::recover` (`START_USER_RECOVERY`, retried while the kernel
still sees the old server, the queue fetched again,
`END_USER_RECOVERY`) lets them go on. A file system mounted on the
device stays mounted across it. The target sees a request twice when
the old server had it and did not answer, which a block device takes.
QS/400 does not set the option. No QS/400 name was in its surface but
its description and a comment.

### Baseline and what was proven

Section 0 at `6d5532fa8`, `QS400_BUILD_ID=buildkit-baseline`:

| Artifact | SHA-256 before |
|---|---|
| `initramfs.cpio` | `4a20ac4f3fd54d7c094bdf334f547ebb6af3165d0eabdf0605f818045fdf83dc` |
| `initramfs-media.cpio` | `8a8bf99c8aeb388c3db9ffdd4b1c01a6f2cd264874a233845dc7e06628e62b31` |
| `uki-a` | `52d1653b5235a47c69c9b7bd66885fb6bb65286392ff050b203d6adb49b0dcf0` |
| `uki-b` | `92ae467278b299c2afdca74b34150ca21503f56b41a295dff7e5ff2dd74983cc` |
| `uki-d` | `72916f9d10b9466d6dc6a6757b11665cb451cfb1dfb57f6689143cd545aee7d3` |
| disk image | `eb10ad38144b4babc4c65eff139c71c8f7da0aa003d45a9debc1b8f480deef5c` |
| ISO | `ede05f6ef7255b12b59b77448061090bae8d7ffa949d7ce059a73ee04a306794` |
| `manifest.json`, `git_rev` masked | `f088c2354162e6e16b04ba94456f5e40b5b295b3c46051b346daf81a34f43bb3` |

As in section 7, the crates this handoff changes are linked by `slic`
and `qsjob` (`console`, `svctools`, `store`, `netlink`, `tcpcfg`), so
their bytes are other bytes with the same code in other crates. What
that leaves to prove was proven in three ways:

- **The pipeline.** The final tree's `initramfs`, `uki`, `disk`, `iso`
  and `manifest` steps over the baseline's userland binaries:
  the seven artifact hashes above again, by `diff` of the sums;
  `manifest.json` differs in `cargo_lock_sha256` only (`Cargo.lock`
  gained `scrkit`, `svcusers` and `protect`).
- **The disks.** `storage/store/tests/sets_bytes.rs` runs fixed
  operations on a mirrored pair (started, resumed part way, suspended,
  a half replaced, a hot spare) and on RAID 5 and RAID 6 sets
  (synchronizing, active, exposed, rebuilding, rebuilt) and digests
  every unit's image at thirteen stops, with the resume map's bytes. The
  digests in `tests/golden/sets.txt` were written by the code before the
  split (commit `1cd01bfc4`, one commit ahead of the baseline and
  nothing but the test); the code after it gives the same, on Windows
  and on Linux. The same bytes from the same operations is both
  directions at once: what the old code wrote is what the new code
  writes and opens, and the reverse. (Object headers and the superblock
  carry the release, so a build of another release runs the scenarios
  and reads everything back without comparing digests.) `fmtcheck` has
  the `store` format at its versions with `protect`'s four format files
  in place of the ones that moved, re-locked (`update --force`, the one
  format touched).
- **The screens.** Every DST, installer, SST and sign-on golden under
  `tools/conformance/golden/` is unchanged and passes; no golden file
  was rewritten.

And the final tree's own `build all` ran under QEMU where the moved
code runs, every step passing: the IPL with its SRC display and the
telnet sign-on (disk steps 1 and 51); from the install suite the
automatic install and the boot of the installed disk (1, 2), a two-disk
install mirrored through the DST screens (4), a power pull during the
synchronization, which went on from the 768 chunks its map had (5), an
IPL with a half missing (6), the half resumed from DST and a service
tools user disabled by three wrong passwords and enabled again (7), the
four-unit mirror with a disk pulled, brought back and resumed with the
system up (12), device parity RAID 5 synchronized across a power down,
exposed with a disk pulled, read through parity and rebuilt (13), and
the hot spare taking a pulled disk's place (16).

### Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Section 0 hashes, `store` tests and screen goldens identical before and after | the seven hashes identical for the final tree's steps over the same userland binaries (`manifest.json` in `cargo_lock_sha256` only); the userland itself differs because the crates it links moved (above); `store`'s integration tests pass unchanged (the unit tests of the parity layout, its field arithmetic and its log went to `protect` with their code); every screen golden passes and none was rewritten |
| 2 | `console` builds without `release`; the screen kit builds without `svctools` | `cargo tree`: `console` reaches no `release`, and with `--no-default-features` no `tn5250` or `ccsid`; `scrkit` depends on `console` only (`tools/buildkit/tests/neutral.rs`, `the_appliance_crates_reach_no_product_crate`); the fixture's PID 1 is built with neither |
| 3 | The protection crate builds without `store`; `store` passes its suite on top of it; old and new sets open in both directions | `protect` depends on `blockdev`, `crc`, `thiserror`, `tracing`; `store`'s suite passes on it, on Windows and on Linux; `tests/sets_bytes.rs` gives the digests the code before wrote at all thirteen stops; QEMU install steps 4, 5, 6, 7, 12, 13 and 16 pass on the final build |
| 4 | `ublk` serves an arbitrary `BlockDev` and survives a server restart | `BlockTarget` over a `MemBlockDev` in `ublk`'s tests; in the fixture under QEMU the mirrored set is `/dev/ublkb0`, its server is killed with a read waiting, and the next server's `Device::recover` answers it under the mounted xfs |
| 5 | `build fixture` passes its three new assertions | at `ec26964c0`: the panel (`QSDEMO Service Tools` under `QSDEMO V0R3M0 fixture`, 12420 bytes lit on the 1280x800 framebuffer, drawn on serial, ended by a terminal's keys), the set (made, written, member 1 removed and the data read back, the server restarted, the member replaced and rebuilt in 96 of 96 chunks, the members identical) and the route (`198.51.100.0/24` with protocol 83) |
| 6 | The extended neutrality test passes | `cargo test -p buildkit --test neutral`: four tests; no `QS400`, `qs400` or `QS/400` outside the tests of `console`, `scrkit`, `svcusers`, `protect`, `ublk` and `netlink` (code, documentation, `build.rs` and `Cargo.toml`), and none of them reaches `release`, `licpart`, `store`, `svctools`, `slic` or `qsjob` with every feature on |

`cargo deny`, `build notices --check` and `tools/fmtcheck` pass with no
version bump.

One commit of crate code came after the fixture's last boot:
`ac98cf2fd` takes the serial display's line for a log
(`AnsiDisplay::describe`) from its caller, so that the text audit's
count does not grow. The fixture does not use it; `console`'s tests
cover it. The fixture was not booted again on it: the build host's disk
filled during that run.
