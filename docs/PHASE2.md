# Phase 2: storage and the object store

What Phase 2 built on top of Phase 1, how it is laid out on disk, how it
recovers, how to drive it, and the evidence for the acceptance criteria.
The byte-level formats are in `STORE_FORMAT.md`; this file is the design
and the operations view. QS/400 is x86_64 only.

## Crates

```
blockdev/     BlockDev trait; LinuxBlockDev (O_DIRECT); MemBlockDev with
              fault injection (fail_after, tear_at, drop_unflushed)
store/        the object store, bottom up:
  src/page/     4 KiB pages with a CRC32C trailer; the Pager trait; MemPager
  src/btree/    copy-on-write B+tree: fixed keys, variable values, bulk load
  src/asp/      units, superblock, logical page space, page I/O
  src/alloc/    bitmaps, free-space tree, free log, checkpoint chunks
  src/wal/      the ring, one record per transaction
  txn.rs        transactions: the Pager trees write through, patches, roots
  store.rs      open with recovery, commit, checkpoint, close, page cache
  cache.rs      CLOCK page cache, write-through
  src/obj/      IDs, headers, the type registry, directories, libraries,
                locks, generic operations, space.rs (byte space over
                extents), dtaara, dtaq (and *USRQ), usrspc, usridx, sysval
  reclaim.rs    RCLSTG
qsname/       10-byte CP037 object names and the special values
ccsid/        CP037 <-> UTF-8 from cp037.csv, EBCDIC collation
diskimg/      the disk layout (ESP, LIC, ASP1), FAT volumes, ISO 9660
              reader, GPT lookup; used by build disk and the installer
slic/         src/ipl/storage.rs (ASP 1 at IPL, QTEMP), sysvals.rs (the
              *SYSVAL object), install.rs (the ISO installer),
              src/svc/objects.rs and storage.rs (console commands)
conformance/  src/phase2.rs: the Phase 2 steps of the disk suite
```

## Design

**Blocks.** Everything goes through `BlockDev`: 4 KiB blocks, `read`,
`write`, `flush`, `discard`. On Linux the backend opens the partition
with `O_DIRECT` and bounces unaligned buffers through `AlignedBuf`;
`flush` is `fdatasync`. Every store test runs on `MemBlockDev`, which
models a volatile write cache: writes are readable at once but reach the
flushed image only on `flush`, and the test can make a write fail, tear
a write at a seeded point, or drop everything unflushed.

**Pages.** A page is a 4064-byte body and a 32-byte trailer: CRC32C,
page type, owning object ID and the writing transaction. The CRC covers
the body and the rest of the trailer. A mismatch is a hard error naming
the page and the owner; nothing reads a bad page silently.

**B+tree.** One generic `BTree<K, V>`: fixed-size big-endian keys (so
byte order is key order), variable values up to 1024 bytes, point get,
floor, range scan, insert, delete with merge or rebalance, bulk load at
90% fill. Copy-on-write: a change rewrites the touched nodes to fresh
pages and the caller publishes the new root. A node the current
transaction allocated is rewritten in place, since nothing else can see
it yet. The property test runs 100,000 random operations against
`BTreeMap` per seed with commits and power pulls mixed in.

**ASP.** An ASP is one or more units concatenated into one logical page
space. Each unit has a unit header at page 0 and a backup at its last
page, and two superblock copies at pages 1 and 2 written alternately by
checkpoint generation; the newest valid copy across all units wins. The
superblock records the creating release, and a store from another V or
R is refused (CPF3294). Phase 2 supports ASP 1 with one unit; the unit
table allows up to 119.

**Allocator.** Between checkpoints the per-unit bitmaps are the truth:
first fit in the unit with the most free space, the largest run when
nothing fits, and `extend` grows an object's last extent in place. A
page freed by a transaction is not reusable until the checkpoint after
it. At a checkpoint the free-space tree is bulk-loaded fresh from the
bitmaps; its pages come from chunks taken from the free runs first, and
the unused chunk tails go into a one-page free log that the next
checkpoint folds back in. Everything a checkpoint writes lands on pages
the previous superblock calls free, so a crash at any point leaves the
previous checkpoint intact. The allocator keeps 1/64 of the ASP back so
a full store can still checkpoint.

**WAL and transactions.** Every change is a transaction. Pages the
transaction allocates (new headers, data pages, tree nodes) are buffered
in memory and written at commit before the WAL record; in-place changes
to existing pages (data area values, queue entries, user space bytes,
header fields, directory roots) are logged as a whole page image on the
first change of a page since a checkpoint and as byte patches after
that. Commit order: the new pages and every record page but the last,
flush every unit, then the last page, flush unit 0. A valid last page
proves the transaction durable. Recovery at open replays every complete
record from the WAL tail, stops at the first incomplete or bad one, and
checkpoints. The ring is 64 MiB; a transaction that would not fit is
refused and the store stays usable; a full ring forces a checkpoint.
Checkpoints also run every 256 transactions and every 30 seconds.

**Page cache.** CLOCK replacement, 256 MiB by default, write-through, so
it never holds dirty committed data. `MIGRATE(phase5)`: the *MACHINE
pool.

**Objects.** IDs are 128 bits: ASP, type code, a serial that is never
reused, and a generation. The object directory (ID to header page) is a
B+tree whose root lives in the superblock and is published by the WAL.
Every object has one header page with the identity, ownership,
timestamps, authority (stored, not enforced), storage (16 inline extents
plus an overflow page) and a 1 KiB type-specific attribute area; the
rest of the page is reserved for the database and journaling phases.
Each library owns a directory tree keyed by type and CP037 name, whose
root sits in the library's attribute area, so a scan lists objects in
EBCDIC order. The registry lists every type from the plan plus the
internal `*SYSVAL`. Every type gets create, delete (needs `*EXCL`),
rename, move, change text, describe and listing; `resolve` walks
`*LIBL`, `*CURLIB` or a named library. Locks are an in-process table
with the standard five-state matrix, held per job handle.

**Data types.** A byte space over an object's extents (4064 bytes per
page) carries `*DTAARA` values, `*DTAQ` and `*USRQ` entries, `*USRSPC`
bytes and the `*SYSVAL` blob; every write is an in-place change logged
through the WAL. Queues append at the tail, receives mark entries dead,
and an emptied queue resets to reclaim its space. `*USRIDX` entries live
in a B+tree owned by the index with a 128-byte key (the entry key
zero-padded plus an insertion sequence, so equal keys keep age order);
find and remove take EQ, NE, GE, GT, LE, LT, and there are first-N and
last-N.

**Names and characters.** `qsname::Name` is the stored 10-byte CP037
form with the unquoted and quoted rules; `Ord` is EBCDIC order. `ccsid`
holds the CP037 table as a committed CSV written from the code page
layout and converts both ways; `ebcdic_cmp` puts `$`, `#` and `@` before
letters and letters before digits.

**RCLSTG.** `rclstg` sweeps every page of every unit in 1 MiB reads,
verifies every allocated page, checks each object's extents, overflow
page and index tree, diffs the object directory and every library
directory against the headers, and reconciles the allocator. `*CHECK`
reports; `*ALL` (and `*SYSBAS`, the same in Phase 2) rebuilds the
allocator from the references, marks objects with bad pages damaged,
moves objects whose library is gone to QRCL, bulk-loads every
directory, clears `rclstg_required` and checkpoints. Liveness of a
header is decided by the allocator: a valid header on a free page is a
deleted object, counted as stale and ignored.

## The IPL

New codes, in the order they appear:

| Code | Phase | Meaning |
|---|---|---|
| C1001070 | LIC | ASP units located (`/dev/...3`, size, O_DIRECT or buffered) |
| C1001080 | LIC | Storage management recovery: WAL replayed, count on serial; a blank unit is formatted here |
| C1001085 | LIC | Storage management recovery failed; followed by B1001040 |
| C1001087 | LIC | Store release mismatch, both releases on serial; followed by B1001050 |
| C6001005 | OS | Object store opened |
| C6001007 | OS | ASP 1 initialized: QSYS, QGPL, QUSRSYS, QSYS2, QRECOVERY, QSPL, QRCL, QHLPSYS (first boot only) |
| C6001008 | OS | Reclaim storage required (`rclstg_required` set) |
| B1001030 | fatal | ASP 1 unit not found on an A or B IPL |
| B1001040 | fatal | Object store unusable |
| B1001050 | fatal | Object store created by another release |
| D1001010 | power down | Object store checkpointed and closed, before the unmounts |

On an A or B IPL the store opens right after hardware enumeration. On a
D IPL it opens after the attended menu, because option 2 may be about to
rewrite the disk; with no ASP unit a D IPL continues without a store.
The run loop wakes every five seconds for the time-based checkpoint.

**QTEMP.** ASP 0 is a 64 MiB `MemBlockDev` formatted at every IPL with
the same code, with a root library called QTEMP. `MIGRATE(phase5)`: one
per job. `QTEMP/NAME` on the service console addresses it.

**System values.** They live in the `QSYSVAL` object (`*SYSVAL`, in
QSYS) as the `sysval` crate's TLV blob. On the first boot after the
store exists, the overrides in the LIC record migrate into the object
and the LIC record keeps only the IPL subset (QIPLTYPE, QPWRRSTIPL,
QRMTIPL, QABNORMSW, QIPLSTS, QCONSOLE, QAUTOCFG) under TLV tag 0x0101;
tag 0x0100 is dropped. Every save writes the object and the subset.
Without a store (media IPL, no disk) the LIC record is used as before.

## Disk layout

`build disk` writes a 4 GiB image by default (`--size` overrides): the
256 MiB ESP at 1 MiB, the 64 MiB `QS400-LIC` partition, then
`QS400-ASP1` (type GUID `3c5a9d7e-2b41-4c6f-9e08-515334304131`) to the
end, 4 KiB aligned. The layout lives in `diskimg`, which the installer
uses on a real disk. The image is sparse; the hashes in `build disk`
and `build manifest` read the whole 4 GiB.

## Install media and the installer

`build iso` puts the media UKI in the El Torito boot image and the slot
A and B UKIs beside it as `QS400A.EFI` and `QS400B.EFI`, since the media
UKI's command line is the D-IPL one. On a D IPL, option 2 of the
attended menu runs the installer. The display shows it as one dialog
screen (title, text lines, a prompt field; a notice waits for Enter
before returning to the menu) and the serial port gets the same text,
so either keyboard answers it:

```
Install the Operating System

Select the disk unit to install on:

  1. DD001  vda       4096 MiB  (QS/400 partitions present)

Selection (0 to return): 1

Install on /dev/vda (4096 MiB)? All data on it will be lost.
Type YES to continue: YES
Licensed Internal Code read from the media (... and ... bytes)
Writing partition table and licensed internal code to /dev/vda (ESP 256 MiB, LIC 64 MiB, ASP 1 3774 MiB)
Initializing the LIC partition
Initializing ASP 1
ASP 1 initialized: 966144 pages, QSYS is 0001-0001-0000000000000001-00000000

Installation complete. Remove the installation media and press Enter to power down.
```

The disks come from `/sys/class/block` (whole disks, no loop, RAM or
optical devices), the resource name from the hardware table. The UKIs
are read from the first optical device whose ISO 9660 volume id starts
with `QS400`. The partition table is written with `diskimg::write_image`
(random GUIDs), the kernel re-reads it (`BLKRRPART`), the LIC record is
initialized with the current system values, and ASP 1 is formatted and
bootstrapped exactly as a first boot would. Then power down; the next
IPL from the disk opens the store (C1001080, C6001005) and reaches
sign-on. No dedicated service tools yet.

## Service console

```
asp                  units, free and available pages, WAL fill, generation, release, flags
lib ls               every library (ASP 1 then QTEMP), EBCDIC order, object counts
obj ls LIB [TYPE]    entries of a library
obj crt TYPE LIB/NAME [TEXT]   create (defaults for *DTAARA, *DTAQ, *USRSPC, *USRIDX)
obj dlt|rnm|mov ...  delete, rename, move
obj dsp LIB/NAME     description, every type of that name
obj bench N          N data areas over QBENCH0..9, created then deleted, rates
dtaara get|set LIB/NAME [VALUE]
dtaq snd|rcv LIB/NAME [DATA]
usrspc rd LIB/NAME OFFSET [LEN] | wr LIB/NAME OFFSET TEXT
rclstg [*CHECK]      reclaim storage, or verify only
chkstg               checksum sweep of every allocated page
txn stats            commits, aborts, checkpoints, WAL pages, images, patches, cache
```

`obj bench` commits a thousand objects per transaction and checkpoints
every four batches, deletes leftovers of an interrupted run first
(untimed), then times creation and deletion.

## Crash tests

Unit level (`store/tests`): the B+tree against `BTreeMap` with crashes
between operations; a torn page write before commit; a corrupted
committed page naming page and owner; the allocator under random
allocation with a torn write at every position of a checkpoint; a
damaged superblock copy; committed transactions surviving a power pull
between commits over forty rounds while uncommitted and aborted work
vanishes; a torn write at every position of a commit yielding the
state before or after, never a mix; each data type across a power
pull; RCLSTG on zeroed directory pages, a bad payload page and a lost
library.

Harness level (`build test`), after the Phase 1 steps:

| Step | What |
|---|---|
| 7 libraries | C6001007 on the first boot; `lib ls` shows the eight system libraries in EBCDIC order |
| 8 sysval in store | QSECURITY is 50 from `*SYSVAL`; `lic` shows tag 0x0101 and no 0x0100 |
| 9 object bench | `obj bench N`, `rclstg *CHECK` clean, `pwrdwn restart`, the next IPL replays 0 |
| 10 power pulls | per seed: boot, `obj bench 200000`, kill QEMU after 1.5 to 7.5 s, boot, C1001080, `chkstg` 0 bad, `rclstg *CHECK` 0 problems, every QBENCH library holds the same multiple of 100 objects, at most 200000 in all |

`build test` runs the bench with 20,000 objects and 3 pulls;
`--bench 1000000 --pulls 10` is the CI size and `--pulls 100` the local
one. `build test --media iso --install` installs onto a blank 4 GiB disk
and boots it.

## Numbers

Measured under QEMU with KVM on the development host (virtio disk, one
vCPU busy), debug harness, release `slic`:

| Bench | Create | Delete | Disk |
|---|---|---|---|
| 20,000 objects | 1941/s (10.3 s) | 5001/s (4.0 s) | 4 GiB image |
| 1,000,000 objects | 2053/s (487 s) | 2793/s (358 s) | 8 GiB image |

A data area is one header page (its value lives in the header when it
is at most 1008 bytes), so a million of them need 4 GiB of headers plus
directories: the million-object bench runs on `build disk --size
8589934592`. On the 4 GiB default image the bench stops with
"no space" after about 700,000 objects. `chkstg` and `rclstg *CHECK`
sweep the 3.7 GiB unit in about 8 seconds.

## Running

```
cargo run -p build -- all                 # dev build; disk and ISO
cargo run -p build -- test                # Phase 1 and Phase 2 disk steps
cargo run -p build -- test --bench 1000000 --pulls 10
cargo run -p build -- test --media iso    # media steps
cargo run -p build -- test --media iso --install
cargo test -p store                       # unit, property and crash tests
```

## Known limits

- One ASP with one unit; the unit table and headers allow more, but
  nothing adds a unit or mirrors (`mirror_of` is stored and ignored).
- No journaling, no authority enforcement, no jobs: headers carry the
  fields; nothing acts on them (`MIGRATE(phase6)`, `MIGRATE(phase5)`).
- No DST menus; the installer is one dialog screen on the display and
  the same text on serial.
- `*USRIDX` entries are at most the key plus 1024 bytes; user spaces at
  most 16 MiB; a queue reclaims space only when it empties.
- Locks are in-process and per caller-supplied job handle.
- Payload writes are always in-place patches; there is no copy-on-write
  relocation of whole pages for large user space writes.
- RCLSTG runs with the store otherwise idle and treats a valid header on
  a free page as a deleted object.

## Acceptance evidence

`build test --bench 1000000 --pulls 10` on the 8 GiB image, 2026-10-10,
QEMU with KVM, 21 minutes 32 seconds in all:

```
PASS 1 SRC sequence              1.6s  13 codes in order, IPL complete at 1.6s
PASS 2 sign-on screen            0.0s  rows 1-10 match the layout
PASS 3 system values             0.0s  QMODEL is QS40; QSECURITY set and read back as 50
PASS 4 restart and history       1.4s  restarted in 1.4s; 2 history entries, first Normal; QSECURITY persisted
PASS 6 screenshot                0.1s  target/conformance/signon.png
PASS 7 libraries                 0.0s  8 system libraries in EBCDIC order
PASS 8 sysval in store           0.0s  QSECURITY from *SYSVAL; LIC record holds only the IPL subset
PASS 9 object bench            866.0s  created 1000000 in 487.17s (2053/s); deleted 1000000 in 358.04s (2793/s);
                                       reclaim check clean; clean restart replayed 0
PASS 5 abnormal IPL              3.0s  C1002000 after a killed IPL; QABNORMSW is 1
PASS 10 power pulls            416.5s  seed 0: pulled after 4.2s, 3 transaction(s) replayed, 11000 objects kept;
                                       seed 1: 7.2s, 1 replayed, 13000; seed 2: 5.5s, 2 replayed, 6000;
                                       seed 3: 1.7s, 2 replayed, 0; seed 4: 4.2s, 3 replayed, 11000;
                                       seed 5: 3.0s, 5 replayed, 1000; seed 6: 1.8s, 0 replayed, 4000;
                                       seed 7: 2.2s, 3 replayed, 3000; seed 8: 6.5s, 0 replayed, 16000;
                                       seed 9: 4.8s, 1 replayed, 1000
PASS, 10 steps
```

Every pull came back with `chkstg` reporting 0 bad pages, `rclstg
*CHECK` reporting 0 problems, and the ten bench libraries holding the
same multiple of 100 objects: only whole committed batches survive.

The same day on the 4 GiB default image: `build test` (bench 20,000,
3 pulls) PASS 10 steps; `build test --media iso` PASS 3 steps; `build
test --media iso --install` PASS 2 steps; `cargo test --workspace` 32
suites green on Windows and Linux; `build deny` clean on both targets
with 157 crates; clippy clean for the host and for musl; `build
userland` verifies `slic` is a static ELF on every build. Unit-level
evidence: `cargo test -p store` runs 67 tests, including the B+tree
property test (three seeds of 100,000 operations plus a dense key
space), the allocator torn-write sweep, the transaction power-pull
rounds, the commit torn-write positions, every data type across a
power pull, and the four RCLSTG scenarios.

## Deviations from the Phase 2 text

- The page CRC covers the trailer's type, owner and generation as well
  as the body, not just the first 4064 bytes.
- The free-space tree is rebuilt by bulk load at every checkpoint from
  the bitmaps rather than edited in place; editing frees its own old
  nodes recursively and ran out of pages on a nearly full ASP. The free
  log holds only chunk tails.
- Large user space writes are patches, not copy-on-write page
  rewrites, so the extent list never changes under a write.
- `*USRIDX` keys are 128 bytes in the tree (120 zero-padded plus the
  insertion sequence); entries beyond key plus 1024 bytes are refused.
- `C1001085` and `C1001087` are informational and are followed by the
  fatal B-codes B1001040 and B1001050.
- `StoreError` reports message IDs through `msgid()` rather than a
  field holding a constant.
- The harness runs a 20,000-object bench and 3 pulls by default; the
  full sizes are flags, because the million-object bench takes minutes.
