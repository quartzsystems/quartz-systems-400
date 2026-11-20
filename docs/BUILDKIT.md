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
| `console` | the 24x80 screen with 5250 attributes, the field editor, the font, evdev keyboards, the SRC display and application screens, on the framebuffer, on a serial terminal and as cells on a partition's console port | its `Identity` (the banner: product, release, build; the code after which a sign-on may show), the system name, the command-line key of the scale |
| `scrkit` | the screen builders and the panel stack | every title, the system name, the function key line, the messages; the environment its panels ask and the outcome they end with |
| `svcusers` | service tools user IDs: salted hashes, privileges, lockout | the users it ships with, and where they are kept (`UserStore`) |
| `protect` | mirrored and parity protection, resynchronization, hot spares, over `BlockDev` units; a set as one `BlockDev` | what identifies a member and records its state; the resume map's keeper; the on-disk constants of its sets |
| `ublk` | a kernel block device served by a process | the device behind it (a `Target`, or any `BlockDev`); whether it outlives its server |
| `netlink` | rtnetlink, sock_diag, nftables, sysctl | the routing protocol number of its routes, the name of its nftables table |
| `qsipart` | the partition service channel's protocol (handoff 17): frames, the service port's and the console port's messages, a peer's grid; `ipc` alone of this repository under it, which `neutral.rs` holds it to | nothing: the port names and the protocol are the partition scope of QSI, both sides' |
| `qsweb` | the web layer (handoff 22, section 17): HTTP/1.1 over any stream, sessions and CSRF, the frame, the partials, the shared pages, golden comparison and the link check, WebSocket | its family (`Brand`: name, vendor, release, `Server` token, the stylesheet's and script's paths, the cookie, the shared pages' policy sentences), each product's wordmark, prefix, navigation tree, header label, routes, messages and help |
| `x509kit` | certificates without a store (handoff 22): P-256, P-384 and RSA keys, self-signed and issued certificates, requests, what a certificate holds, PEM chains and keys matched, PKCS#12 read and written | the names, the validity, the passwords |
| `termlook` | the terminal's look (handoff 23, section 18): the console's font and palette, its cell rules and status row, a screen drawn to PNG and Print Screen's PDF, fed from a 5250 screen or a partition console's `qsipart` grid; the page's script as bytes and the exchange it speaks on its socket, keys by name to `qsipart`'s; `qsipart` and `ipc` alone under it | the system's name on the status row, the connection's state and TLS, the scheme and scale, the PDF's producer, the keypad, the script element's paths |
| `srcdata` | every reference code the system posts (handoff 23): family, phase, text, meaning, and cause and recovery where written (`gaps` counts the rest); `slic` posts from it; nothing under it | nothing: the codes are the system's |

### Sets for a storage pool (handoff 15)

QS/VS builds its storage pools on `protect`'s sets. Three questions its
pools depend on, answered from the code as it is; one change came of
them (the copy with a cursor, under 3).

**1. Does the partial-parity log bound the repair at open to what was
in flight?** Yes, on RAID 5 and RAID 6, with and without a failed
member.

- Every write to a row of a protected stripe is held until a flush and
  made in epochs of at most `PPL_ENTRIES` (126) stripes
  (`Engine::flush`, `epoch`). An epoch writes and flushes its partial
  parity on each stripe's parity members (P, and Q on RAID 6) before
  any of its data or parity pages is written.
- At open, `ParitySet::open` runs `Engine::recover`: it reads the two
  slot headers of each member that takes writes, takes each one's
  newest valid slot, and repairs only the stripes those slots name
  (`repair` reads one stripe and writes its parity). A member's newest
  slot may be older than the last epoch when it held no parity in it,
  so the bound is members × 126 stripes: at the most, 18 members, 2,268
  stripes, about 160 MiB read, whatever the size of the set. Nothing
  else at open reads the data.
- With a failed member: its slots are not read, but each entry is on
  every parity member of its stripe, so on RAID 6 the other copy is
  read; a RAID 5 stripe whose parity member failed has no parity to
  repair. A failed member's page that the epoch did not write is
  computed from the entry's partial parity (`gf::solve` over its P and
  Q). The store's `tests/parity.rs` cuts an epoch after its data and
  opens with every unit, and without one.

What reads more than what was in flight:

- **A stale member is rebuilt whole, unless the set keeps maps.** A
  member recorded failed that is back, or that the others' state pages
  record failed after the caller's record (`recorded_failures`), opens
  `Rebuilding(0)`: every other member is read for it. A set opened with
  a `ParityIntent` (handoff 16, section 12) keeps a map per member from
  the moment it fails, set before each write while it is away, and
  rebuilds only the stripes of its regions.
- **A synchronization mark behind** is taken from the caller's record,
  so the stripes between it and the engine's mark at the cut are
  computed again: bounded by how often the caller records the mark. A
  new set's synchronization reads everything once, as it must.

One case is bounded but not exact. When a member had failed before the
cut, an interrupted epoch wrote a page of that member (its content is in
the parity only), and another data page of the stripe landed while the
parity did not, the repair computes the failed member's page from the
parity on disk and the new data: neither its old nor its new content.
The entry holds the parity of the pages not written, not the content
of a written page whose member cannot take it. The page was not
acknowledged (no flush returned). Closing it: the entry also carries the
new content of such a page, one more page an entry and only while a
member is failed, and the repair writes it back into the parity.

**2. Can a set grow?** No, neither kind.

- `MirrorSet`: the size is fixed when it opens. `replace` and
  `take_spare` accept a larger device and the set keeps its size; no
  call adds a member. Growing would be modest work: a further member is
  one pushed `Resuming` and resumed; a larger size, once every member
  has it, needs the new tail made the same on each member (copied or
  zeroed, else a block never written reads differently from each), the
  resume and intent maps sized again, `ublk::Device::resize` (the IFS
  volume already grows that way) and the file system's grow
  (`XFS_IOC_FSGROWFSDATA`, which `ublk`'s `sys.rs` issues).
- `ParitySet`: the `Geometry` (level, members, blocks, head) is made
  once by `plan` and kept "whatever the size of a member replaced
  since"; a larger replacement's extra pages are not used. Each member
  holds its rows, then the parity pages, then the two log slots and the
  state page, and the set presents the members' rows end to end
  (`locate`), so more blocks or another member moves data and parity:
  a reshape, with a progress mark and a backup of the window being
  moved, which the crate does not have.

So a pool's size is fixed when it is made, unless QS/VS asks for
mirrored growth.

**3. Large sets.** No limit binds below what follows; memory does not
grow with the set except where said.

| Where | Limit |
|---|---|
| `protect`, mirrored | blocks are `u64` (4 KiB); members up to 65,535 (`u16` unit numbers in `Halves`) |
| `protect`, parity | 18 members (`MAX_MEMBERS`: `u32` failure masks, `u8` member numbers); blocks `u64`; 126 stripes an epoch; up to 4,096 pages (16 MiB) held between flushes; synchronization and rebuild buffers grow with the step's budget, not the set |
| `ublk` | size in 512-byte sectors, `u64`; one queue of 32 tags, 512 KiB a request (`DEPTH`, `MAX_IO`): throughput, not size |
| `xfsfmt` | formats at most 64 TiB (`MAX_BYTES`: 1 TiB allocation groups, 64 of them). A larger pool is formatted at 64 TiB or less and grown by the kernel's xfs, which has no such limit, or the constant is raised after testing a larger layout |

Memory per TiB of a mirrored set:

| What | Per TiB | When |
|---|---|---|
| the intent map's bits, 16 MiB regions | 8 KiB (65,536 regions) | always, with a map |
| a dirty region's counters | about 50 bytes each | only regions set |
| a resume's or a copy's chunk map | 128 KiB (one bit a MiB) | while one runs |

Two costs grew with the size of a set before this handoff, and one
still does:

- `copy_step` looked at every chunk from the first on each call, and
  `ResumeState::complete` and `percent` count every chunk: a resume one
  chunk at a time was quadratic in the set's size: for one TiB, a
  million chunks, about 10^12 bit tests, tens of minutes of processor
  time; for 16 TiB, 256 times that. `intent::copy_next` does the
  same copy (the same reads, writes, flushes and saves, in the same
  order; a test compares them) with a cursor and a count of what is
  left; `MirrorSet` uses it for resumes and for the copy after an
  unclean stop. The store keeps `copy_step`, unchanged: its sets are
  the disks of one machine, and changing it would change `slic`.
- Still: a resume saves the whole chunk map through `ResumeJournal`
  every 16 chunks, 128 KiB a TiB each time: about 13% more writes than
  the copy itself at 16 TiB, half as much again at 64 TiB. A journal
  for a large set keeps the bytes it last wrote and writes only those
  that changed (a few bytes a save); the trait gives it the whole map,
  so this is the caller's choice and needs no change here.

## 6. Crates another repository takes

`build fixture` builds twenty-two of them as git dependencies (all but
`srcdata`, data QS/MC takes and `slic` posts from), and its PID 1
runs `console`, `scrkit`, `protect`, `ublk`, `netlink`, `qsipart`,
`qsi`, `qsisess` and `qsitrust` (a pairing over TLS and an `identify`),
and `qsweb`, `x509kit` and `termlook` (a web product of its own over
TLS, with a terminal page).
What an
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
| `console` | product-neutral (section 10). With `default-features = false` it is the screen, the editor, the font, the keyboards, the SRC display and the three displays (the framebuffer, `ansi`, and `qsi`, the grid as cells on a partition's console port, section 13), and depends on `qsipart` and `ipc` only of this repository; the default feature `session` adds the sign-on display and the 5250 terminal, with `tn5250` and `ccsid` |
| `scrkit` | product-neutral: `ui` and `panel` over `console` |
| `svcusers` | product-neutral: `UserDb`, `Privilege`, `UserStore` and `FileStore`; `ipc` with the feature `wire` |
| `protect` | product-neutral: over `blockdev` only. It writes no label of its own (section 10); a mirrored set's write-intent map is kept where the caller says (section 11) |
| `ublk` | product-neutral: `Device`, `Target`, `BlockTarget`, `Options` |
| `netlink` | product-neutral: nothing of this repository under it |
| `qsipart` | product-neutral: the partition service channel's protocol (`docs/QSID.md`), both ports' frames and messages, the console's cells, a peer's `Grid`; `ipc` alone of this repository under it (`neutral.rs`), a fuzz target (`qsipart`). QS/VS pins it |
| `qsi` | product-neutral: QSI's messages (`docs/QSI.md`): the hello, the families with their versions and capability bits, every request, reply and event, `qsi::any` by number, `qsi::samples`; `ipc` and `qsipart` of this repository under it (`neutral.rs`). QS/VS and QS/MC pin it |
| `qsisess` | product-neutral: QSI's session as a state machine over any byte stream (handshake, requests with replies, events with a replay log, streams with credit, keep-alive, limits); no sockets, no TLS; `ipc`, `qsipart` and `qsi` under it; three fuzz targets (`qsisess-frames`, `-handshake`, `-streams`) |
| `qsitrust` | product-neutral: QSI's trust rules: mutual TLS 1.3 over graviola with pinned self-signed Ed25519 certificates (QS/VS's identity, byte for byte), and pairing by SPAKE2 bound to both certificates and the connection; keeps no state (`Pins`, `pair::Guard` are the caller's); nothing of this repository under it |
| `qsweb` | product-neutral (section 17): the web layer of QS/400's three web products and of QS/MC's; nothing of this repository under it (`neutral.rs`); the stylesheet keeps its file name `qs400.css` and its bytes |
| `x509kit` | product-neutral (section 17): keys, certificates, requests, PEM bundles, PKCS#12; nothing of this repository under it. `certs`, QS/400's certificate store, is built on it and re-exports its modules |
| `termlook` | product-neutral (section 18): the look, the screen model with its two ways in, PNG and PDF, the script and its exchange; `qsipart` and `ipc` under it (`neutral.rs`); its one line naming QS/400 is the console font's file name it reads. Access for Web's `s5250::look` and `s5250::screen` re-export it |
| `srcdata` | product-neutral (section 18): the reference codes as data; nothing of this repository under it. QS/MC explains a partition's code with it |
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

## 11. Handoff 15: the write-intent bitmap of a mirrored set

A mirrored set that lost power copied a whole member again, because
nothing recorded which regions were being written (QS/VS
`docs/OPEN_QUESTIONS.md` item 19). `set::MirrorSet` now keeps a
write-intent map when the caller asks for one. QS/400 does not: `store`
uses `Halves`, `copy_step` and `ResumeState` and never `MirrorSet`, no
file `slic` links was changed, and the units of an ASP are the same
bytes. The three answers QS/VS asked for are in section 5.

### The map

`MirrorSet::open_with(id, blocks, members, Some(intent))` keeps it;
`open` is `open_with(.., None)` and behaves as before. An `Intent` is:

| Field | What | Default |
|---|---|---|
| `magic` | eight bytes of the caller's, first in the kept bytes | (the caller's) |
| `region_blocks` | blocks a region, a multiple of the 1 MiB resume chunk | 4,096 (16 MiB) |
| `clear_delay` | how long a region stays set after its last write | 5 s |
| `fresh` | a new set, its members alike: the map is written clean; otherwise it is read, and one missing or not this set's (another magic, version, set, size or region) takes every region as dirty | `false` |
| `store` | where the map is kept: `intent::IntentStore`, two calls, `read_all` and `write(offset, bytes)` durable when it returns | (the caller's) |
| `away` | for each member, a store for a map of its own while it is out of service (below) | none |

The crate still writes no label. The bytes are the caller's to keep
where it keeps the set's metadata; `intent::IntentMap` is the map in
memory and its versioned form, which the caller may use:

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | magic, the caller's |
| 8 | 4 | version, 1 |
| 12 | 4 | kind: 0 none, 1 the set's map, 2 a member's map from when it left in step, 3 a member's map from when it left while resuming |
| 16 | 8 | blocks a region |
| 24 | 8 | regions |
| 32 | 16 | the set's identity |
| 48 | 16 | the member's identity (zeros for the set's map) |
| 64 | regions / 8, rounded up | one bit a region, region 0 the low bit of the first byte |

A bit is changed in place: setting or clearing it writes the bytes that
hold it and nothing else. `intent::HEADER_LAYOUT` is the table, and a
test holds them to each other. The form is not under QS/400's format
freeze (`tools/fmtcheck`), since QS/400 writes none; it carries its
version.

### Writes, flushes, clearing

- **A write** into a region whose bit is clear sets the bit and writes
  it durably (one `IntentStore::write` of the byte, under the set's map
  lock) before the write goes to any member. If that write fails, so
  does the set's write, and no member has seen it. A write into a
  region already set costs nothing more than a count. A write that
  touches two regions sets both in one store write.
- **A flush** sent to the members takes an epoch first; when it has
  reached every member that takes writes, each region whose writes all
  ended before that epoch and none since may be cleared.
- **Clearing is lazy and batched.** At each flush (and at `sweep`, for a
  caller's timer when the set goes quiet), the regions that may be
  cleared and have had no write for `clear_delay` are cleared together
  in one store write. A region written again within the delay keeps
  its bit: a steady workload sets each region it works in once and
  pays nothing more; the test `a_region_written_again_and_again_is_set_once`
  makes one store write for a hundred writes and flushes. A clear that
  fails leaves the bits set in memory and the whole map is written
  again before the next bit is set.
- **`close`** flushes and clears every bit that can be: the map of a
  set stopped cleanly has no region set.

So the price of the map is one durable write of the caller's store for
the first write into a region that has been quiet for the delay, and
one for each batch of regions cleared; nothing on the read path, except
during a copy (below).

### After an unclean stop

The map is read at open; its set regions may differ between the
active members. With two or more active members, `resync_step(budget)`
copies those regions, a chunk at a time, from the first active member
to the others: the resume's copy (`intent::copy_next` over the same
`ResumeState` map, with every chunk of a clean region marked done), and
like a resume it holds the members' lock for one chunk, so the caller's
I/O goes on between chunks. Until a region is copied, its reads are
served by the first active member only, so a block reads the same
whichever member would have served it. A region is taken as in step
once its chunks are copied and flushed; its bit is then cleared as any
other, and until then it stays set, so a further power loss copies it
again. `resync_progress` gives the chunks copied, the chunks dirty at
open and the chunks in the set. A target that fails is taken out of
service and the copy goes on; a source that fails is taken out and the
next active member is read.

### A member out of service

A member that stops taking writes (suspended, detached, failed) while
the set has a store for it in `Intent::away` gets a map of its own:
the set's bits at that moment, then every region set while it is away,
never cleared. When it comes back (`resume`, after `reattach` with the
same disk, or at a later open with its device and its record
`Missing`, `NotOperational` or `Suspended`), the resume copies only the
chunks of that map's regions, and the map is given up (kind 0) once the
member is active. A member that left while resuming has a map of kind 3:
the resume's own map, if the journal kept it, copies those regions
again; without it, a full copy. A resume the journal kept after an
unclean stop also copies again the regions the set's map had at open.
A member with no store, a store that held no valid map, or a map that
could not be written, comes back by a full copy, as before. `replace`
(another disk) gives the member's map up.

### Proof

`storage/protect/tests/intent_cut.rs` runs a scenario of writes and
flushes over two members (writes into clean and dirty regions, across a
boundary, a region written again after its bit was cleared) with a
count of durable steps (each member's writes and flushes and each store
write) shared by the members and the store, and cuts the power at
every step: 51 steps, each with members that cache writes until a
flush and with members that write through. Two cuts each: the devices
fail from then on and the set sees it (it takes a member out, which
the caller records), or the process dies where it stands (the device
panics; the caller's record still has both members active). After each
cut the caches are dropped, the set is opened again from the map as
kept, the copy runs, and a member the set took out is resumed. Each
time: the members are identical, every block holds what the last
acknowledged flush made durable or a value written since, the copy read
the first member once per chunk it copied and never the whole set.
Of the deaths, 17 left the members apart with both still recorded
active, and the copy alone mended each (92 copies in all, none of the
whole set); of the failures, 17 left them apart and the resumes of the
member taken out mended them (34 resumes). Two changes to the code fail it: a write
that does not set its bit, and a bit cleared before the flush passed.

`storage/protect/tests/intent.rs`: only the dirty regions are copied,
the target is not read and reads of a dirty region come from the
source until it is copied; one store write for a hundred writes into a
region, both regions of a write across a boundary in one, all bits
cleared at `close` in one; a write whose bit cannot be kept is refused
before any member sees it; no map kept (or another set's) means every
region; a copy cut short by a further power loss goes on with the
regions it had not copied; members suspended, detached and failed come
back from their own maps (four chunks for two regions), a member with
no map by a full copy; a resume the journal kept copies again what was
written after the stop. `intent::copy_next` does what `copy_step`
does, call for call, over maps with chunks done here and there and
budgets of 1 to 100.

**The fixture.** QSDEMO's PID 1 keeps its set's map in a file
(`/data/set.intent`, magic `QSDEMOWI`, 1 MiB regions so that its 96 MiB
set has regions enough to tell a few from all, cleared at each flush),
written with `pwrite` and `fdatasync` (`FileIntent`). After its earlier
proofs it serves the set again with both members active, writes and
syncs 1 MiB files without a pause, kills the server in the middle of
it, and starts the next one on the same device (`Device::recover`),
which opens the set from the map and runs the copy beside the I/O. The
proof fails unless some region was dirty, the copy took fewer chunks
than the set has, every file reads back as last written (and a file
written before the load too), and the two member files are identical
once the set is stopped. `build fixture` asserts its line ("server
killed under write load: C of 96 chunks copied (D dirty), N files
intact; the members are identical") and passed at `dbc7e52a0`. The
step keeps the console in memory, so C, D and N of that run are not
recorded here.

### Baseline and what was proven

Section 0 at `73437f304`, `QS400_BUILD_ID=buildkit-baseline`:

| Artifact | SHA-256, before and after |
|---|---|
| `initramfs.cpio` | `b3acae8b634d25535a78d3d7a668116980217434bc0c17061c231171aef35967` |
| `initramfs-media.cpio` | `a60b32bb3efe52d7b9d87246c0a7f0c60be0576ae6e0384c6f0cbb1151b4141e` |
| `uki-a` | `07413abbacd85481c16455a88214085c3b24a55304e67414644deed850aca9fe` |
| `uki-b` | `cc661cb6f9c3d95684ec3922255e76cd0d13aeb98b3525de44a1e1f5628702d1` |
| `uki-d` | `4710df49ad0e8fc8af66d66d7ca327c4ce239a0049be06925d47a7dadd6aa5d0` |
| disk image | `dbae024e15d735df1a3bb18ed92671a06c9060891e2a0e7f1c77468454f1ff7a` |
| ISO | `ddc6cd3c612deb8b8aefc4767f1dab38d15a01edb8cd92e77e96c3e074539e34` |
| `manifest.json`, `git_rev` masked | `5ae5750595d6c7b7e032544063b6ee15ddafcdf166a587c2f112167d6fef360a` |

The final tree's `build all` (`dbc7e52a0`) gives the same eight, `slic`
and `qsjob` included: unlike handoffs 13 and 14, no proof over the
baseline's userland was needed. The `store` suite gives the same 159
results before and after, `tests/sets_bytes.rs` among them. `build
fixture` passed at the baseline and on the final tree.

The first build after the change did not give them: `initramfs.cpio`
and what holds it differed, through `slic` alone (same size, 6 MB of
bytes moved). Built twice with its symbols, the two `slic` binaries had
the same 25,913 functions but one: `core::slice::sort` for `u64`, which
`sql` instantiates, was now `protect`'s instance, because the map's
open built a `BTreeSet<u64>` with `collect` (which sorts); under fat
LTO the instance linked moved, and the code after it. The map builds
its sets by inserting now (`dbc7e52a0`), and the two `slic` binaries
are byte for byte the same. What `slic` never calls can still change
it, through a generic of the standard library it shares with a crate
`slic` does call.

### Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Section 0 hashes and the `store` suite identical before and after | the eight hashes identical for the final tree's own `build all`; the `store` suite's 159 results identical |
| 2 | The fault-injection tests pass | `tests/intent_cut.rs` (51 steps, two cuts, cached and write-through members) and `tests/intent.rs`, with `intent::copy_next`'s test against `copy_step`; two broken versions of the map fail them |
| 3 | `build fixture` passes its new assertion | at `dbc7e52a0` |
| 4 | Section 2's three answers are in this document | section 5, "Sets for a storage pool" |
| 5 | The neutrality test still passes | `cargo test -p buildkit --test neutral`: four tests; `protect` names no product |

`tools/fmtcheck` reports every format the same with no version bump;
`Cargo.lock` did not change, so `deny` and the notices are as they were.

## 12. Handoff 16: what QS/VS Phase 5 needs

QS/VS finished its Phase 5 (storage pools, virtual disks, virtual
optical and tape, IPL source D) pinned at `0fc8afa50`. Four things it
found belong here; all four are done.

| Item | Done | Where |
|---|---|---|
| 1. `NR_CPUS` to 512 | yes, alone (no kernel bump is planned) | `kernel/config/qs400.config` |
| 2. Reflink in `xfsfmt` | yes, by option, off by default | `ifs/xfsfmt`, `docs/IFS.md` "Disk format" |
| 3. Tape past the early warning | yes, against `st.c`; the drive checks are listed below | `media/tape`, `lic/slic/src/stgsvc/media/drive.rs`, `docs/MEDIA.md` |
| 4. A returning parity member rebuilt from a map | yes, by option | `storage/protect/src/set/` |

### 1. NR_CPUS 512

`CONFIG_NR_CPUS=512` in its own commit, without a bump of
`kernel/VERSION` (6.18.55 stays). 512 is the most x86_64 takes without
`MAXSMP` and off-stack CPU masks (`NR_CPUS_RANGE_END`); the resolved
`.config` has `NR_CPUS=512`, `MAXSMP` not set. Measured in WSL (KVM,
12 host threads), the same tree built twice:

| | `NR_CPUS=64` | `NR_CPUS=512` |
|---|---|---|
| `bzImage` | 12,801,024 bytes | 12,796,928 bytes |
| `uki-a` / `uki-d` (media) | 32,416,768 / 13,046,784 | 32,416,768 / 13,042,688 |
| `vmlinux` text, data, bss | 37,811,647, 10,321,093, 2,711,552 | 37,866,622, 10,342,361, 2,662,400 |
| QEMU start to slic's first SRC, `-kernel`, 4 vCPUs (median of 10) | 2.73 s | 2.78 s |
| the same, 12 vCPUs | 2.86 s | 2.86 s |
| step 4, restart to the prompt | 15.3 to 17.7 s (3 runs) | 16.7 s |
| processors at `-smp 96` (`hw`) | 64 (PN01 to PN64) | 96 (PN01 to PN96) |

The kernel grows 55 KB of text and 21 KB of data; compressed, nothing
(the tables `NR_CPUS` sizes are zeros in the image, and the per-CPU
areas are allocated at boot for the processors present). Boot time is the same within the noise (a few runs of each
took 5 s, both kernels alike). 512 costs nothing noticeable. QS/VS sets
the same value in its `qsvs.config` at the pin that carries this.

### 2. Reflink in xfsfmt

`Options::reflink` (`false` from `Options::new`) adds the reverse-mapping
and reference-count btrees and sets the RMAPBT and REFLINK ro-compat
bits. With it off the bytes are those of before: `tests/identity.rs`
holds a CRC-64 of eleven whole images made by `297771eec` (64 MiB to 64
TiB, 512 and 4096 sectors, zeroed or not, fixed AG sizes), and the ten
standard images of `verify.sh` have the same SHA-256 before and after.
With it on, the images match `mkfs.xfs -m rmapbt=1,reflink=1` (6.18)
byte for byte at 64 MiB (both sector sizes), 200 MiB, 1 GiB and sixteen
AGs, except where they already differed (secondary superblocks, inode
times and generations). What changes per AG, the root inode (128) and
the log's minimum are in `docs/IFS.md`, "Disk format".

Acceptance, as root in WSL (kernel 6.18.33, the pin is 6.18.55):
`tools/conformance/xfs/verify.sh check` over 22 images (64 MiB, 512
MiB, 1 GiB, 20 GiB, an odd size, sixteen AGs, one AG; both sector sizes;
reflink on and off): `xfs_repair -n` clean before and after a mount
that creates, fills, links, renames and deletes; on the reflink ones
`cp --reflink=always` and an `xfs_io reflink` of a range, `fiemap`
showing the extents shared (flag 0x2000), a write to the copy leaving
the original's SHA-256 as it was and its own extent no longer shared,
sharing kept across a remount; no kernel messages. `verify.sh big` does
the same at 1 TiB and 64 TiB (sparse files on tmpfs, since ext4 stops
at 16 TiB). The output stays deterministic: a function of the size and
the options alone.

**What it costs.** On disk, four blocks an AG (two btree roots, two
more free-list blocks): 16 KiB an AG. At mount the kernel reserves room
for the two btrees to grow to their worst case, and that shows as used:

| Size | Free (statfs), reflink off | Free, reflink on | Cost |
|---|---|---|---|
| 20 GiB | | | 379 MiB (1.85%) |
| 1 TiB | 268,022,265 blocks | 263,150,076 blocks | 18.6 GiB (1.82%) |
| 64 TiB | 17,162,479,900 blocks | 16,850,643,106 blocks | 1,190 GiB, 18.6 GiB a TiB |

So about 18.6 GiB a TiB, reserved, not written: the btrees take it as
files share and unshare extents.

**64 TiB stays** (`MAX_BYTES`: 64 AGs of 1 TiB). Reflink does not
change it, and the 64 TiB images mount, clone and repair clean.

Two things found on the way:

- **One AG.** With `ag_blocks` set, a device shorter than two AGs gets
  one AG, and the log in it overlapped the first inode chunk: a
  filesystem that could not mount. Fixed: the free list and the chunk
  now follow the log, as `mkfs.xfs --unsupported -d agcount=1` places
  them (equal byte for byte, reflink on and off). Only that layout's
  bytes changed. Such a filesystem has no secondary superblock, so
  `xfs_repair` needs `-o force_geometry` until it grows to two AGs.
  `slic`'s IFS volume always has several AGs.
- **AGs of 2^28 blocks.** From 4 TiB up, xfsfmt makes AGs of exactly
  2^28 blocks (1 TiB); mkfs.xfs 6.18 makes at most 2^28 - 1. The kernel
  and `xfs_repair` take them; they are left so, since changing them
  would change the bytes of existing callers.

### 3. Tape past the early warning

Read against Linux 6.18.55's `drivers/scsi/st.c` and
`Documentation/scsi/st.rst`, QS/400's path past the early warning was
wrong in a way that would have failed every save across volumes on a
real drive. What `st` does there, now in `docs/MEDIA.md` as a table:

- The write that reaches the early warning succeeds with its whole
  count. The next write fails `ENOSPC` with nothing written, the one
  after it succeeds, and so on alternately. `GMT_EOT` is set only after
  a write that saw the warning; after the refused one `MTIOCGET` shows
  `GMT_EOD`, not `GMT_EOT`.
- At the physical end (VOLUME OVERFLOW) a write fails `ENOSPC` with
  nothing written, and again when tried once more.
- With asynchronous writes (variable blocks, no direct I/O) the warning
  comes on the next call, which may be `MTWEOF` or `MTIOCGET`: it fails
  `ENOSPC` and does nothing.
- `MTWEOF` past the warning succeeds; at the physical end it fails
  `EIO`. After writes, `st` writes a filemark itself before `MTREW`,
  `MTOFFL`, `MTSEEK`, `MTBSF` and at close.
- Write protection is `EROFS` at an `O_RDWR` open or at `MTLOAD`, and
  `EACCES` for a write on a node opened read-only. After `MTOFFL` every
  call answers `ENOMEDIUM` until `MTLOAD` or a new open.

What was wrong, and is fixed:

1. The code took the warning to be a failed write with `GMT_EOT`, and
   so ended the volume at once.
2. Every refused write past the warning was taken as the end of the
   medium; EOV2 always falls on one, so the volume switch never
   worked. Now a call that fails `ENOSPC` is done again once: success
   is a block past the warning, a second `ENOSPC` the end. No block is
   written twice.
3. `MTWEOF` and `MTIOCGET` failing `ENOSPC` after an asynchronous write
   get the same one retry; `MTWEOF` failing `EIO` past the warning is
   the end of the medium.
4. Nothing loaded the next cartridge: the transport now sends `MTLOAD`
   after its own unload (`EROFS` there: loaded but protected, which the
   writer refuses), and the drive's provider in `slic` unloads the full
   cartridge, asks the operator, and asks again while the drive is
   empty.
5. A rewind or unload whose automatic filemark fails at the end is
   tried again after `MTWEOF 0` (a flush).
6. Fixed blocks: a read asks for one block, a write must be one block.

The trailer is `TM EOV1 EOV2 TM TM` (IBM: two tapemarks after the
trailer labels at end of volume), as before; the next volume has VOL1,
HDR1 with the section number one more, and the data goes on. Reading
joins the volumes at EOV1 and the next HDR1.

`tape::st::sim::StSim` is a model of `st` (its state machine and
return codes over cartridges in memory: variable and fixed blocks,
asynchronous writes, the automatic filemark, protection, unload and
load, short counts in fixed mode). `media/tape/tests/st_eov.rs` checks
the model against `st.rst`, each block written once past the warning
(variable, asynchronous, fixed, read back), filemarks past the warning
and at the end, protection at open and at load, a labeled file across
two and three cartridges record for record (HDR1 numbering, the
continuation, EOV and EOF block counts, the joined data, read back by
volume list and by `*MOUNTED`), a cancelled request, a protected next
cartridge and the wrong one. `media/medio/tests/st.rs` saves and
restores through `medio` across two and three cartridges and one
cartridge short. The replay fixture `st_session.txt`, written by hand,
had `st` wrong and is corrected.

**For QS/VS's Phase 10, on its drive.** What only a drive settles:

1. Variable blocks into the warning, no filemark between: n, `ENOSPC`,
   n, `ENOSPC`. The drive must report EOM (NO SENSE, 00/02, residue 0)
   on the crossing write and every write after.
2. On to the physical end: `ENOSPC` twice in a row (VOLUME OVERFLOW,
   sense key 0Dh, EOM, residue the whole block), nothing written.
3. `MTWEOF` past the warning succeeds and clears `GMT_EOT`; at the end
   it fails `EIO`.
4. The AWSTAPE file write-protected: `open(O_RDWR)` fails `EROFS`, a
   read-only open works with `GMT_WR_PROT`, a write gets `EACCES`.
5. `MTOFFL`, another AWSTAPE file, `MTLOAD` on the same node: succeeds,
   file and block numbers back to 0; `MTLOAD` with no cartridge fails,
   then `ENOMEDIUM`; `MTLOAD` of a protected one on an `O_RDWR` node
   gives `EROFS`.
6. Reading back across the warning reports no EOM; a filemark reads 0
   with `GMT_EOF`; end of data reads 0 right after a filemark, else
   `EIO`, with `GMT_EOD`.
7. Spacing: `MTFSF` past end of data `EIO` with `GMT_EOD`; `MTBSF` past
   BOT `EIO` with `GMT_BOT`; `MTFSR` into a filemark `EIO` with
   `GMT_EOF`; `MTEOM` gives `GMT_EOD`.
8. Whether 64 KiB blocks go by direct I/O or asynchronously; 1 and 3
   both ways (`MTSETDRVBUFFER MT_ST_ASYNC_WRITES`, `try_direct_io=0`),
   where the warning shows on the next `MTWEOF` or `MTIOCGET`.
9. Room past the warning for one 64 KiB block and `TM EOV1 EOV2 TM TM`,
   plus a full driver buffer with buffered writes.
10. INZTAP, SAVLIB over two and three AWSTAPE volumes answering the
    next-volume inquiry (and once replying before loading: asked
    again), RSTLIB, compare; ENDOPT(*UNLOAD) and a fresh open after.
11. After an end-of-medium failure, `MTREW` works (the `MTWEOF 0`
    path).
12. Not used by QS/400, only to see `st` is sane: `MTIOCPOS` (only
    `StTransport::logical_block`, which nothing calls), `MTSEEK` (never
    sent), `MTSETDENSITY` 0x58 (`*ULTRIUM5`), block size 0 as variable.

The model leaves out fixed-mode buffered writes losing data at the end
(`st` returns `EIO`: an I/O error here), read-ahead, partitions,
setmarks, and a short count in variable mode (an error here).

### 4. A parity set's maps of members out of service

A RAID 5 or RAID 6 member recorded failed that came back opened
`Rebuilding(0)`, and every other member was read for it (section 5).
`set::ParitySet` now keeps a map for each member out of service, as
`MirrorSet` does (section 11), when the caller asks for one. QS/400 does
not: `store` drives `parity::Engine` and never `ParitySet`, and no file
under `parity/` changed.

#### The map

`ParitySet::open_with(.., Some(intent))` and `create_with` keep them;
`open` and `create` are the same calls with `None` and behave as
before. A `set::ParityIntent` is:

| Field | What | Default |
|---|---|---|
| `magic` | eight bytes of the caller's, first in the kept bytes | (the caller's) |
| `region_blocks` | blocks of the layout (`Geometry::blocks`) a region: one region is the same rows and parity pages on every member | 1,024 (1,024 × n pages of each member: 16 MiB of each member of a set of four) |
| `away` | for each member, an `intent::IntentStore` for its map | none |

The bytes are `intent::IntentMap`'s, kind 2 (`Away`), the member's
identity in them; `HEADER_LAYOUT` and the version did not change. A
QS/VS pool disk keeps its member's map beside its label: one bit a
region and 64 bytes of header: 4 KiB of bits for a set of four 512 GiB
members at the default region.

#### Its life

- **Leaving.** When a member's device fails I/O, the set writes the
  member's map before the engine goes on without it: every member's
  device is wrapped, and its error keeps the map first. The map is the
  regions written since the last flush that completed, and from the
  mark on when the member was being rebuilt whole. `detach` flushes and
  keeps the map before the member is taken out. A member recorded in
  step and missing at open, that the others did not record failed, gets
  the regions of the stripes in the partial-parity slots.
- **Away.** Each write sets its regions' bits in every map kept, durably,
  before any member sees it. Nothing is cleared.
- **Back.** At open, a member with its device and a valid map opens
  `Rebuilding(0)`, whatever mark was recorded (it may have failed again
  after the mark), and `step` rebuilds only its regions: the slots are
  zeroed, then the mark jumps over the blocks outside the map (they are
  in step) and the engine's `rebuild_step` runs over each region. Once in
  step, the map is given up (kind 0). `away_regions(member)` shows it.
- **No map.** A member with no store, a store holding no valid map, or
  a map that could not be written, comes back by a full rebuild, as
  before. `replace` (another disk) gives the map up; a replacement that
  fails during its own rebuild gets a map from its mark on.

What keeps it honest: whenever the kept bytes are a valid map, every
stripe outside its regions holds the member's page in step. So the bits
are written before the header that makes them a map; a bit that cannot
be written gives the map up (its header off) and the write goes on; and
when the header cannot be written either, the write is refused before
any member sees it. A map the set could not give up is kept and set as
before, a superset. A caller that opens a set with maps after opening it
without must give the maps up first: they would miss those writes.

#### What it does not change

- **RAID 5, a member failing during its own rebuild.** The engine cannot
  lose a member being rebuilt in RAID 5 (`can_lose` counts it), so the
  flush that meets it fails and the set fails I/O until it is opened
  again. The map holds every region the member is not in step for, so
  the next open rebuilds it from there. Fixing it is in `Engine`, which
  `slic` links.
- **Without maps,** a member recorded `Rebuilding(mark)` that failed
  again after the record still opens at its mark. With maps it starts
  over the map.
- **QS/400.** The new code is in `set/` and `intent::IntentMap::next_set`,
  which `store` does not call; it builds no `BTreeSet` and sorts
  nothing (the lesson of section 11). `slic` changes in this handoff
  anyway (item 3), so its bytes were not compared.

#### Proof

`storage/protect/tests/parity_intent_cut.rs` runs a scenario of writes
and flushes: member 1 fails in a flush with writes held for it, writes
while it is away (into new regions and its own rows), then (RAID 6)
member 3 too; the set is opened again with them back, writes go into
regions of the map and outside it during the rebuild, and after it. A
shared count of durable steps (each member's writes and flushes and each
map store write) cuts the power at every step, with members that cache
writes until a flush and with members that write through, two ways: the
devices and stores fail from then on, or the process dies where it
stands. After each cut the caches are dropped, the set is opened again
as the caller recorded it, and the work runs. Each time: every member
ends in step, `verify` finds no stripe out of step, every block holds
what the last acknowledged flush made durable or a value written since,
and a rebuild from a map read only the blocks of its regions.

| Level | Steps | Runs | Rebuilds from a map | Full rebuilds | Blocks read by the map rebuilds |
|---|---|---|---|---|---|
| RAID 5, 4 members, 128 blocks, 16 regions | 141 | 568 | 458 | 44 (all after a store failure) | 17,376 of 58,624 (30%) |
| RAID 6, 5 members, 102 blocks, 13 regions | 411 | 1,648 | 1,455 | 148 (all after a store failure) | 64,016 of 148,410 (43%) |

The full rebuilds all follow cuts where the stores fail with the
devices, so no map is kept: correct, and what a mirrored set does too.
Two changes to the code fail it (a stripe's parity out of step after
the rebuild): a write while a member is away that does not set its
bit, and a map made at a failure without the regions held for the
flush. A third, an open that keeps the recorded mark of a member with a
map, fails `tests/parity_intent.rs`. A fourth is caught by neither: not
zeroing the member's slots before its mark jumps. A stale slot does harm
only at a later open where another page of its stripe is unknown (the
repair otherwise reads every page); the set zeroes them, as the engine
does at mark 0.

`storage/protect/tests/parity_intent.rs`: a member back is rebuilt in
its two regions only, RAID 5 and RAID 6; a member failing in a flush
keeps what was in flight; two RAID 6 members away come back each by its
map, one while the other is still away; a member failing during its own
rebuild comes back by the same map (RAID 5 and 6, the failure recorded
or only the mark before it); a replaced member is rebuilt whole and its
map given up, and the replacement failing part way gets a map from its
mark; no map kept means a full rebuild; a map that cannot be kept is
given up or the write refused; a member missing at open gets a map of
what was in flight, kept across opens while it stays away.

### Signatures

Nothing QS/VS calls changed: `MirrorSet` (`open_with`, `resync_step`,
`resume_step`, `sweep`, `close`, `take_spare`, `replace`,
`drain_events`, `members`), `ParitySet` (`create`, `open`, `step`,
`work`, `sync`, `take_spare`, `replace`, `geometry`, `drain_events`,
`members`), `intent` (`IntentMap`, `IntentStore`, `MapKind`,
`HEADER_LAYOUT`, the version) and `xfsfmt::format` are as at
`0fc8afa50`. Added:

| Crate | Addition |
|---|---|
| `protect` | `set::ParityIntent { magic, region_blocks, away }` and `ParityIntent::new`; `ParitySet::open_with(.., Option<ParityIntent>)`, `ParitySet::create_with(.., Option<ParityIntent>)`, `ParitySet::away_regions(member)`; `IntentMap::next_set(from)` |
| `xfsfmt` | the field `Options::reflink`; `Geometry::reflink`, `Geometry::for_features(bytes, sector_size, ag_blocks, reflink)`, `agfl_count`, `agfl_blocks(agno)`, `reflink_roots`, `rmap_records(agno)`; `RmapOwner` |
| `tape` | `MtOp::Load` (so `MtOp::ALL` is `[MtOp; 12]`); the module `st::sim` (`StSim`, `SimCartridge`, `SimRecord`, `MARK_LEN`); `StTransport::unload` when already unloaded is `NotLoaded` |

One addition can break a caller: `Options::reflink` is a new public
field, so a struct literal `xfsfmt::Options { .. }` without `..` must
name it. Code built on `Options::new` (all of QS/400's) does not change.

## 13. Handoff 17: the partition service channel

QS/VS gives each partition two virtio-console ports, a service channel
and a console port. QS/400 now speaks the partition scope of QSI on
both; the record of the protocol and of both sides is `docs/QSID.md`.

| Item | Done | Where |
|---|---|---|
| 1. The protocol | yes: frames, versions and capabilities, both ports' messages, a peer's grid, a fuzz target; `ipc` alone under it | `lic/qsipart` |
| 2. `qsid` in a partition | yes: started before the first code; hello, codes, state; `power_down`, panel 21, ping, the clock; a `B1` code then a halt | `lic/guestagent/src/qsid/`, `lic/slic/src/procs/qsid.rs` |
| 3. The console on the console port | yes: `QsiDisplay`, the keys back, `ConsoleMode::Partition` | `lic/console/src/qsi.rs`, `lic/licpart`, `lic/slic/src/ipl/display.rs` |
| 4. Proof | yes: QEMU steps 195 to 198, the fixture | `tools/conformance/src/partition.rs`, `tools/buildkit/fixture/init/src/panel.rs` |
| 5. Docs | yes | `docs/QSID.md`, `docs/QUALIFIED_HARDWARE.md`, this section, `CLAUDE.md`, the release notes |

### What QS/VS pins

`qsipart` at the commit that carries this section. It takes `ipc` and
`thiserror`; `tools/buildkit/tests/neutral.rs` holds it to that
(`the_partition_protocol_takes_ipc_alone`) and lists it among the crates
an appliance takes. The fixture's PID 1 builds against it from git and
draws its panel through `console::qsi::QsiDisplay` over a port in
memory, its peer's `Grid` checked against the screen.

Detection is `hwqual::checks::qsvs`: the SMBIOS manufacturer `Quartz
Systems` and product `QS/VS`, the FADT's hypervisor vendor `QUARTZVS`,
or the OEM IDs of every ACPI table. QS/VS must give at least one of
them; the QEMU steps use the SMBIOS line.

### Acceptance

| # | What | Result |
|---|---|---|
| 1 | hashes and goldens outside a partition | the goldens unchanged; `instload` identical; `slic` and `qsjob` change, by the crates they link (below) |
| 2 | the protocol suite | `cargo test -p qsipart` passes |
| 3 | the console suite | `cargo test -p console` passes, the Linux service tests too |
| 4 | neutrality, the fixture | `cargo test -p buildkit --test neutral` passes; `cargo run -p build -- fixture` passes |
| 5 | the fuzz target for 60 s | 267,778 runs, 300 in the corpus, 363 edges, no finding |
| 6 | `QSID.md` describes every message | sections 4 and 5, each kind with its fields |

The userland built twice in one directory with one build ID
(`QS400_BUILD_ID=h17cmp`), at `a9d5c8a4f` and after. A build's bytes
depend on its directory, so the two must share one:

| Binary | Before | After |
|---|---|---|
| `slic` | 49,333,288 bytes, `f2836b03148eae0b` | 49,407,784 bytes, `99a52428588debab` |
| `qsjob` | 41,769,512 bytes, `a6b0ac190afdf1ac` | 41,769,576 bytes, `bac751e06cfb237d` |
| `instload` | 586,040 bytes, `276b03e2120e9ed3` | the same |

`qsjob` links `console`, `licpart` and `svctools`. Built with those
three as at `a9d5c8a4f` and the rest of the handoff in place, it is the
old binary byte for byte, so its 64 bytes are theirs: the third console
mode, the console's interrupt and resume, and the console mode panel
refusing an option it does not show. Outside a partition none of them
runs. `slic` grows by the daemon, its start and the partition's paths.
The kernel and its configuration did not change.

QEMU, steps 1, 2, 114 to 117 and 141 (outside a partition) and 195 to
198 (in one) pass; what each partition step shows is `docs/QSID.md`
section 10.

### Two things found on the way

- **The first hello.** The daemon said hello before `slic` had sent it
  any state, so a peer attached at the start was told an empty system
  name. `slic` now sends the state right after it starts the daemon,
  the daemon waits up to five seconds for it, and a hello goes again
  whenever its facts change (the system's name, release, serial, slot,
  IPL type or keylock).
- **A panel request during the IPL.** Queued, it would have run when
  the IPL ended. The daemon refuses function 21 itself before the IPL is
  complete and at a normal keylock; `slic` is asked only when it can
  show DST.

The fixture's mirrored set under load failed once ("the load left no
region dirty") and passed on the next run: its check depends on the
load being in flight when the server is killed, and nothing in this
handoff touched it.

## 14. Handoff 18: QSI's crates

QSI, the protocol a management console speaks to what it manages, is
three crates both QS/VS and QS/MC pin (`docs/QSI.md`):

| Crate | What | Of this repository under it |
|---|---|---|
| `qsi` | the hello, the families (base, pair, a QS/VS host's nine), their versions, capability bits and messages | `ipc`, `qsipart` |
| `qsisess` | the session as a state machine over any byte stream | `ipc`, `qsipart`, `qsi` |
| `qsitrust` | mutual TLS 1.3 with pinned Ed25519 certificates over graviola, and pairing by SPAKE2 | nothing |

`neutral.rs` holds each to that list (`the_protocol_crates_take_only_their_own`,
which replaced the test of `qsipart` alone) and to naming no product.
`build fixture` takes all three from git and its PID 1 pairs a console
with a managed side and asks `identify` (two `QSDEMO qsi:` lines). The
fixture's QEMU now runs with the host's processor (`-cpu host`, `max`
under TCG): graviola needs AVX2, ADX, AES and PCLMULQDQ, which QEMU's
default model lacks, and panics without them. A QS/VS host or console
that runs QSI needs such a processor too.

Also in the handoff, for QS/VS: TLV `0x010C` in the LIC record, the
service channel a system has (`docs/QSID.md` section 11), so the
hypervisor need not wait for a channel a system lacks (QS/VS's open
question 38). It is the one change to what QS/400 does.

### What changed in QS/400's artifacts

Built in one directory with one build ID, the base `5ce33f14b` and the
end of the handoff:

| Binary | Base | After |
|---|---|---|
| `slic` | 49,407,784 `2f4f7e96…` | 49,414,696 `1b12d318…` |
| `qsjob` | 41,769,576 `2354c7ef…` | the same |
| `instload` | 586,040 `276b03e2…` | the same |

`slic` grows for two reasons, each shown by putting its files back:
with `lic/osobjects/shipped/NOTICES.txt` from the base it is 49,408,744
bytes (`5c08e373…`), so 5,952 bytes are the notices of `spake2` and the
three crates it brings (the notices are made from the whole
`Cargo.lock` and compiled into `osobjects`), and with `slic`'s two IPL
files (`ipl/lic.rs`, `ipl/install.rs`) from the base as well it is the
base's binary byte for byte, so the other 960 are the TLV's write. The
shipped `NOTICES.txt` itself is the other artifact that changed. The
screens' goldens (`svctools`, `console`, and the others' crates were
not touched) pass unchanged, and `fmtcheck` is green: the TLV and QSI's
three formats are drafts (`lic-service-channel`, `qsi`, `qsi-session`,
`qsi-pairing`) until QS/VS and QS/MC ship against them.

The fixture passed whole at `183b8ec84`. Of the six runs before it, the
QSI lines held in every one at the revisions that print them, and the
mirrored set's check under load ("the load left no region dirty",
section 13) failed in four: it needs a write in flight when the server
is killed, and the host's processor may finish more of the load in its
1.5 seconds. Nothing in this handoff touched the set.

## 15. Handoff 19: QSI's servicing

QS/VS's next phase serves problems, code update of the host and the
host's own dumps, and QS/MC's Service Management pages are their client.
The messages are in `lic/qsi` and the one thing QS/VS's server waited for
is in `lic/qsisess` (`docs/QSI.md` sections 4, 5, 7 and 10):

| What | Where | Capability |
|---|---|---|
| `EventLog::resume(limits, next, events)`: a log taken up after a restart numbers on, with no placeholders (QS/VS's open question 42) | `qsisess::EventLog` | none |
| problems: list, get, close; opened, seen again, cleared, closed | `qsi::service`, family `0x0003` | bit 2, `SERVICE` |
| code update: status, stage (an upload stream), discard, install, activate, cancel; staged, verified, progress, installed, activating, confirmed or rolled back | `qsi::update`, family `0x0004` | bit 3, `UPDATE` |
| the system's own dumps: list, take, delete, download (a stream); taken, removed | `qsi::base`, operations `0x0010` to `0x0013`, events `0x0010` and `0x0011` | bit 4, `SYSTEM_DUMPS` |

`caps::SERVICING` is the three bits; `caps::HOST_V1` is unchanged. Since
base's dumps are operations of a family that every peer has, the session
now checks a request against `caps::of_request(family, op)` at both ends
and sends an event only to a console that has `caps::of_event(family,
op)`: an older console is never asked or sent what it lacks, and its
event numbers skip what it is not sent. No listener is added to QS/400.

### Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Artifact hashes and existing goldens identical | `Cargo.lock` unchanged, and nothing QS/400 ships depends on `qsi` or `qsisess` (only the host fuzzer does). Built in one directory with one build ID at the base `30e18c2e5` and at `bea634071`: `slic` 49,414,696 `6f738864…`, `qsjob` 41,769,576 `d4458189…`, `instload` 586,040 `276b03e2…`, the same at both. No golden's crate was touched |
| 2 | `EventLog::resume` tested | `log::tests::resumed_from_what_was_kept` (the bounds, a push after, every refusal); `tests/events.rs` `a_resumed_log_numbers_on_and_replays_what_it_was_given` through two sessions |
| 3 | Every new message round-trips; a peer without a bit is never sent its family | `qsi`'s `tests/families.rs` (every sample, bounds, open values, `what_each_message_needs`); `qsisess`'s `tests/samples.rs` (73 exchanges, every event, in pieces) and `tests/caps.rs` (both directions of age, raw requests refused `QSI0003`, events filtered and replays counted) |
| 4 | Neutral; the fixture builds; fuzz 60 s clean | `neutral.rs` passes; `build fixture` passed whole at `bea634071` (its first run there passed both `QSDEMO qsi:` lines and failed only the mirrored set's check under load, the flake of section 14); the three `qsisess` targets ran 60 s each with no finding (`docs/QSI.md` section 12) |
| 5 | `docs/QSI.md` describes the three message by message | section 7 (base's system dumps, service, update), sections 5 and 10 (bits, identifiers), section 8 (the mapping is QS/VS's to complete), "left for later" down to physical I/O |

`fmtcheck` is green: the `qsi` draft format now lists `service.rs` and
`update.rs` and its lock was updated (a draft needs no version bump).

## 16. Handoff 20: QSI's physical I/O and a running partition's entitlement

QS/VS's next phase gives a host's devices to partitions, passes a
physical tape drive through, and changes a running shared partition's
entitlement. The messages are in `lic/qsi` (`docs/QSI.md` sections 5,
7, 9 and 10); `lic/qsisess` is unchanged but for its tests' hellos:

| What | Where | Capability |
|---|---|---|
| a host's devices: released, assignable or why not, the isolation group, the holder; released, taken back, assigned, returned, reset failed; a profile's `physical_io` acted on, refusals `QSI1301` to `QSI1307` | `qsi::host`, operation `0x0010`, events `0x0010` to `0x0014`; `host::physio` | bit 17, `PHYSICAL_IO` |
| the host's physical tape drives (read only), a profile's tape device naming one as `@` and its location code, `PhysicalTape` 3 in a partition's `Drives` | `qsi::vres`, operation `0x0034`, event `0x000A`; `model::MediaDevice::drive`, `tape_source` | bit 17 |
| a running shared partition's units, capping and weight; the change in `Current`, as `EntitlementChanged` and as partitions' `ResourcesChanged`; refusals `QSI1401` to `QSI1405` | `qsi::host::partition` version 2, operation `0x0007`, event `0x0001` | bit 18, `PARTITION_V2` |

`caps::HOST_V1` and `caps::SERVICING` are unchanged. Virtual
processors, dedicated processors and memory still change only at
activation. No listener is added to QS/400.

### Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Artifact hashes and existing goldens identical | `Cargo.lock` unchanged, and nothing QS/400 ships depends on `qsi` or `qsisess`. Built in one directory with one build ID at the base `39105ca27` and at `1f9b50aa2`: `slic` 49,414,696 `5ee963fc…`, `qsjob` 41,769,576 `d6d0e444…`, `instload` 586,040 `276b03e2…`, the same at both. No golden's crate was touched |
| 2 | Every new message round-trips; a peer without a bit is never sent it | `qsi`'s `tests/families.rs` (every sample, the bounds of the entitlement and of a drive's location, `Blocker` and `DriveKind` open, `what_each_message_needs` by bit, a tape device as volume, drive or empty); `qsisess`'s `tests/samples.rs` (76 exchanges, every event, in pieces) and `tests/caps.rs` (16 new requests and 20 new events across both ages, raw requests refused `QSI0003`, and `each_bit_alone`: each of bits 2, 3, 4, 17 and 18 alone at either end lets through its own and none of the others') |
| 3 | Neutral; the fixture builds; fuzz 60 s clean | `neutral.rs` passes; `build fixture` passed whole at `1f9b50aa2` on its first run; the three `qsisess` targets ran 60 s each with no finding (`docs/QSI.md` section 12) |
| 4 | `docs/QSI.md` describes the three message by message | section 7 (the host's physical I/O, vres's physical tape drives, partition version 2), sections 5 and 10 (bits, identifiers), section 8 (the mapping is QS/VS's to complete), section 9 (the decisions), "left for later" brought up to date |

`fmtcheck` is green: the `qsi` draft's lock was updated (a draft needs
no version bump; no file was added).

## 17. Handoff 22: the web layer and certificates

QS/MC's only management surface is a web UI in the design system of
QS/400's three web products (`docs/WEB_STYLE.md`). `net/qsweb` was that
layer already, but QS/400's: it depended on `release`, printed `QS/400`
and fixed QS/400's paths. And a console needs to make and read
certificates without QS/400's certificate store, which depends on
`licpart`.

| What | Where |
|---|---|
| The family a product belongs to: name, vendor, release, the `Server` token, the stylesheet's and the navigation script's paths, the session cookie's name, the sign-on notice and where a product's notice goes, the User field's length, the Not authorized page's note, whether the error page offers Job log | `qsweb::brand::Brand`; `frame::Product::brand`. QS/400's three products each declare `BRAND` with today's values; `qsweb` no longer depends on `release` |
| The `Server` header from the caller | `Handler::server`; `Response::write` takes it; `httpd` passes `QS400/` and the release |
| The session cookie by name | `Product::session_id`, `set_cookie`, `clear_cookie`; `session::set_cookie(name, id, path, secure)` |
| A body streamed instead of held (added) | `Handler::streams` and `handle_stream`; `http::request::read_head`, `read_body`, `BodyStream` (length or chunked, no limit of the layer's); `http::multipart::Parts`, a form's parts one at a time; under `serve_guarded` the body reaches the request's thread through a channel the panic hook closes. Downloads already streamed (`Body::Stream`, `Body::Sized`) |
| The limits, documented | `qsweb::http`'s module documentation: head 16 KiB, a body read whole 8 MiB, 200 requests a connection, time limits are the caller's socket timeouts |
| Golden pages from outside | `golden::Normalize` takes the navigation script's path; `golden::crawl` and `resolve`, the link check over the caller's fetch |
| Certificates without the store | `net/x509kit`: `crypto` (keys, randomness, hashes, PBKDF2, the PKCS#12 KDF), `x509` (`CertInfo`, `build`), `pem`, `pkcs12`, moved from `certs` unchanged but for the error type (`x509kit::Error`, the variants `certs::CertError` maps one to one); new `server::self_signed` (a P-256 key and a server certificate for the names and days given), `server::request`, `bundle::read_pem`, `read_pkcs12`, `key_matches`. `certs` keeps the store, the stash, the CA, the applications and AES-GCM and Argon2, and re-exports `x509`, `pem`, `pkcs12` and `crypto` so every path is as it was |

**PKCS#12.** Yes: `x509kit::pkcs12` reads and writes PKCS#12 files in
pure Rust (PBES2 with AES, and the legacy PKCS#12 ciphers 3DES and RC2
through the RustCrypto `des` and `rc2` crates), and passes `deny`; it is
what `certs` imports a PKCS#12 file with.
`x509kit::bundle::read_pkcs12(data, password)` gives the certificate,
its chain in order and the key, with the key matched.

**The fixture.** Its PID 1 serves a web product of its own, `QSDEMO
Management Console` in the family `QSDEMO`, over TLS 1.3 on a socket
pair with a certificate `x509kit` made, and a client pinned to it signs
on, has a forged form refused, posts the settings form with its CSRF
token and gets the `QMC2001` message row; the page must match
`init/golden/settings.html` (the release in it is `@RELEASE@`).
`build fixture` asserts the four `QSDEMO web:` lines, and that the
fixture's PID 1 is built without `certs` either.

### Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Every existing golden page and artifact hash is identical | The three products' golden and behaviour tests pass unchanged (`cargo test -p httpadmin -p navigator -p accessweb`). Beyond the goldens' token comparison, every response the three products' test browsers got (124,641: status, headers and body, session tokens blanked) was recorded at the base `423c65dea` and at `987fcf87c`, and again after the sign-on fields: 124,557 byte-identical; the 72 HTTP Server Administration responses that differ are identical once key material is masked (serial numbers, fingerprints, PEM and request bytes, PKCS#12 downloads of keys made in each run); Navigator's are all identical; Access for Web's 12 are one test that polls a background copy (`copy_to_table_and_back`), whose requests vary from run to run at the base too. Built in one directory with one build ID (`buildkit-baseline`): `instload` 586,040 `276b03e2…` at both; `slic` (49,603,304 `707ddb08…` to 49,651,432 `8b34aeb0…`) and `qsjob` (41,884,200 `5d26ee20…` to 41,926,088 `1858462a…`) differ because they link `certs`, `qsweb` and the three products, whose code moved or gained the brand; `Cargo.lock` gained `x509kit` and two edges, `NOTICES.txt` is current (no crate outside the tree was added). The pipeline's code (`buildkit`, `tools/build` but the fixture step) is unchanged |
| 2 | `qsweb` builds without `release`; the certificate crate without `licpart` | `qsweb` depends on no crate of this repository, `x509kit` on none either (`neutral.rs`, `the_protocol_crates_take_only_their_own`) |
| 3 | `build fixture` serves and checks its page over TLS with a certificate it made | passed at `987fcf87c` and again at `2f9a3c309` (after the sign-on fields): `QSDEMO web:` the certificate (P-256, self-signed, made by `x509kit::server`), TLS 1.3 to a pinned client, sign-on, a forged form refused, `QMC2001` shown, the page matching `init/golden/settings.html`; the step asserts the four lines and that the fixture's PID 1 has no `certs` in its graph |
| 4 | The neutrality test passes for both crates | `tools/buildkit/tests/neutral.rs`: `the_web_crates_name_no_product` (sources, templates and manifests; the one line allowed is `include_bytes!("../assets/qs400.css")`, the stylesheet's file name) and the dependency check above |
| 5 | The PKCS#12 question is answered | yes, above: `x509kit::pkcs12::read` and `bundle::read_pkcs12` |

Also: `cargo clippy` clean for every touched crate and for `slic` and
`qsjob` on the musl target; `build deny` and `build notices --check`
pass; `fmtcheck` green (no frozen format's file was touched: the
certificate store's `store/format.rs` stays in `certs`). `textaudit` and
`kcbuild notes` report the same as at the base (literals in `lic/` and
the V1R1M0 marker inventory, neither from this work).

**After the first cut** (QS/MC's sign-on golden): `Brand::user_max`
(the User field's `maxlength`), `Brand::notice_in_form` (a product's
notice as the sign-on table's first row instead of a `<p>` above the
form), and an empty `signon_notice` leaving the `(C)` line alone. QS/400's
three give `10`, `false` and their line, and HTTP Server
Administration's recorded responses are again identical to the base's
but for key material; `qsweb`'s `signon_as_the_brand_says` renders both
forms.

## 18. Handoff 23: the terminal's look and the reference codes

QS/MC shows a partition's console in a browser, drawn exactly as Access
for Web draws a 5250 session, and explains a partition's reference code.
Access for Web's look was its own (`net/accessweb/src/s5250`, fed by a
5250 data stream), and the codes were `slic`'s.

| What | Where |
|---|---|
| The look | `net/termlook` (`look::font`, `look::render`, `file::{png, deflate, pdf}`), moved from Access for Web unchanged but for what a console cell has and no 5250 byte gives (yellow, high intensity, double width; `render::Width`, `Style::high`, `Scheme::foreground_of`) and the PDF's producer from the caller. Access for Web's `s5250::look` and `s5250::screen` re-export it |
| One screen model | `termlook::screen::Screen`: size, cells (`Paint`), the cursor's shape, the status row (`Oia`), the look; `picture`, `png`, `print_screen`, `size`, `text_rows` |
| Two ways in | `screen::s5250` (the 5250 screen and `View`, `Screen::from_buffer` the conversion the session job did itself) and `screen::grid` (a `qsipart::console::Grid` cell for cell; the attribute map is `ACCESSWEB.md` "The look for a product outside QS/400") |
| The script and its exchange | `termlook::script` (`SOURCE`, `text(keypad)`), `assets/term.js`; `termlook::exchange` (`screen`, `status`, `closed`, `message`, `style_char`, `Input::parse`, `Input::console_keys`, `keys`). The script element's attributes and the exchange are documented in `ACCESSWEB.md`; without `data-socket` the script is Access for Web's page as before |
| The reference codes | `lic/srcdata` (`Src` with `meaning`, `cause`, `recovery`; `Family`, `TABLE`, `lookup`, `gaps`); `slic`'s `src.rs` is `pub use srcdata::*` |

**The codes without cause and recovery** (`srcdata::gaps`, asserted by
its test; `PHASE1.md` "Reference codes as data"): 74 codes, every one
with its meaning; 64 lack a cause, a recovery or both.

| Family | Codes | Without cause | Without recovery |
|---|---|---|---|
| C1 | 15 | 8 | 15 |
| C6 | 34 | 28 | 33 |
| A6 | 12 | 0 | 7 |
| B1 | 7 | 0 | 3 |
| D1 | 6 | 6 | 6 |

**The fixture.** Its web product gains `/demo/terminal`
(`init/src/term.rs`): QSDEMO's menu drawn by the console's own `qsi`
display into `qsipart` frames, the frames applied to a peer's `Grid`,
the grid drawn by `termlook` as `screen.png`, which must be
`init/golden/terminal.png` byte for byte (made from the same module, the
fixture's own). The page carries the keypad form and the script's
element (`data-socket`, `data-image`); `/demo/terminal/script.js` is
`termlook::script::text`; the keypad's posted `F3` comes back as
`Function(3)` in a `QMC2002` message row, and a socket's
`{"t":"key","k":"Enter"}` decodes to `Key::Enter`. `build fixture`
asserts the line `terminal: qsipart frames to a grid, the picture
matches its golden PNG`.

### Acceptance

| # | Check | Result |
|---|---|---|
| 1 | Every existing golden page, picture and artifact hash is identical | Access for Web's golden pages and its seven pictures pass unchanged (`cargo test -p accessweb`, `s5250_look` byte for byte); the session page's `<script>` element is as it was, so its golden pages did not change. Built in one directory with one build ID (`buildkit-baseline`) at the base `155aba213` and at `3fba6cf55`: `instload` 586,040 `276b03e2…` at both; `slic` (49,651,432 `8b34aeb0…` to 49,670,792 `7b7e4bae…`) differs because it posts from `srcdata`, whose codes now carry their meaning, cause and recovery, and `qsjob` (41,926,440 `3c4a5036…` to 41,938,088 `5a0be7e7…`) because it links Access for Web over `termlook` (the exchange, the new script). `Cargo.lock` gained `termlook` and `srcdata` and their edges; `NOTICES.txt` is current |
| 2 | A `qsipart::Grid` draws through the look crate; the sign-on comparison is recorded | `accessweb/tests/console_look.rs`: `QDSIGNON` as one data stream, drawn as the session job draws it and as console frames into a grid, byte-identical pictures at scale 1 and 2, connected and off. QS/MC's `5250-24x80.png` is Access for Web's Work with Active Jobs with another user name; with it, all seven of QS/MC's pictures are drawn byte for byte (`ACCESSWEB.md`) |
| 3 | The script's exchange is documented and Access for Web runs on it unchanged in behaviour | `termlook::exchange` and `ACCESSWEB.md`; Access for Web's server encodes and decodes through it, its WebSocket and script tests pass; the script's 5250 extras (keyboard map, hotspots, cursor placing, Copy and Paste) are on without `data-socket` as before and behind `data-5250` otherwise |
| 4 | `slic` posts from the reference code crate; the count without cause and recovery is recorded | above; the code, text and phase of all 74 compared old to new, the serial line, the LIC history bytes and `srcs table` from the same `line`, `bytes` and `Phase` |
| 5 | `build fixture` passes its terminal page; the neutrality test passes | `build fixture` passed at `4c0dec595` with the terminal line; `neutral.rs`: `termlook` reaches `qsipart` and `ipc` only, `srcdata` nothing; `the_look_and_the_codes_name_no_product` (the one line allowed is the console font's file name `termlook` reads) |

Also: `cargo clippy` clean for `termlook`, `srcdata`, `accessweb`,
`buildkit`'s tests and the fixture's PID 1, and for `slic` and `qsjob` on
the musl target; `fmtcheck` green; `build notices --check` current.
`kcbuild notes` reports the V1R1M0 marker inventory stale as at the base;
this work moved one marker in `httpafw/s5250.rs` (line 734 to 716).
