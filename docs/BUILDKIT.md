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
product, built and booted by `build fixture`).

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

## 6. Crates another repository takes

`build fixture` builds all ten as git dependencies. What an appliance
should know of each:

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
| `console` | QS/400's: it prints `release`'s banner (QS/400's release when pinned from elsewhere), and brings `tn5250` and `ccsid` |
| `svctools` | QS/400's: the DST and installer screens, over `console`, `licpart`, `release` and `hwqual` |

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

`console` and `svctools` stay QS/400's, as section 6 says.
