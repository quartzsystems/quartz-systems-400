# The integrated file system

Phase 10 gives QS/400 IBM's integrated file system: one namespace with
root, `/QOpenSys`, `/QSYS.LIB`, `/QOPT`, character devices and
user-defined file systems, IBM's object types, CCSID tagging, the IFS
authority model, share modes, byte-range locks, check-out, auditing,
and the operations behind the APIs (`open()` ... `Qp0lGetAttr`) and the
commands (`WRKLNK` ... `RCLLNK`). The crate `ifs/ifs` is all of it below
the job and service plumbing; it runs on Windows over an in-memory
backend, which every test uses.

| Module | Does |
|---|---|
| `errno` | IBM `errno` values (exactly `QSYSINC/SYS(ERRNO)`), Linux mapping, messages, `strerror` texts |
| `path`, `path::fold` | parsing, limits, components; case folding for `CASE(*MIXED)` |
| `backend` | `trait Backend` over a real directory tree; `MemBackend`, `LinuxBackend` |
| `meta`, `meta::codec`, `meta::reclaim` | the one metadata record per object, its encoding, `RCLLNK` |
| `auth`, `auth::data`, `auth::need` | IFS data/object authorities, the search, what each operation needs |
| `vfs`, `vfs::dev`, `vfs::qsys`, `vfs::optical` | the mount table and the view file systems |
| `resolve`, `resolve::index` | the one resolver and the per-directory casefold index |
| `lock`, `lock::share`, `lock::range` | share modes, byte-range locks, the lock table |
| `text` | text-mode (`O_TEXTDATA`) conversion state |
| `ops` | `Ifs<B>`: every operation, per-owner descriptors, audit hooks |
| `save` | the `SAV` walk and per-object descriptor |

The storage service owns one `Ifs<LinuxBackend>` over the xfs mounted
at `/run/qs400/ifs`; jobs reach it through `stg` requests and never hold
an xfs descriptor. Every call takes a `Ctx`: the job's `Owner` (one per
connection), its `Principal`, an `AuthEnv` (profiles, UID/GID,
authorization lists, `QCRTOBJAUD`, answered from the store), an `Audit`
sink and the job CCSID. When a job's connection closes the service calls
`Ifs::release_owner`, which closes its descriptors (writing back member
buffers), ends its directory streams and drops its locks.

## Namespace

| Path | Id (`st_vfs`) | What | Names |
|---|---|---|---|
| `/` | 1 | root: backend `root/` | fold, keep case |
| `/QOpenSys` | 2 | backend `QOpenSys/` | exact |
| `/dev` | 3 | character devices, one `QASPnn` directory per ASP with UDFS | fold |
| `/QOpenSys/dev` | 4 | character devices | exact |
| `/QSYS.LIB` | 5 | the object store through `QsysView` | IBM's object naming |
| `/QOPT` | 6 | optical volumes through `OpticalView` (none until Phase 11), read-only | fold |
| `/QNTC`, `/QFileSvr.400`, `/QDLS` | 7, 8, 9 | the directory exists; anything else is `ENOTSUP` (`CPFA0AD`) | |
| a `MOUNT` point | 100 and up | a UDFS: backend `UDFS/QASPnn/NAME.udfs/` | as created |

`Ifs::ensure_layout` makes the backend's top directories and the
standard directories where they are missing, owned by `QSYS`:
`/home` (`*PUBLIC *RX`), `/tmp` (`*RWX`, restricted rename and unlink),
`/QIBM`, `/QIBM/ProdData`, `/QIBM/UserData`, `/etc`, `/QOpenSys/usr`,
`/QOpenSys/etc`, `/QOpenSys/tmp` (`*RWX`, restricted). The backend's
`UDFS/` area is reachable by no path; a UDFS shows as the block special
file `/dev/QASPnn/NAME.udfs`.

Mount points replace whatever their parent directory holds under that
name; `readdir` of the parent lists them. Mounts made by `MOUNT` last
until the next IPL, as IBM's do.

### Resolution

`resolve::Sys::resolve` walks a path component by component from `/`
or the owner's current directory. It keeps the canonical path (stored
case, no links) as it goes:

- `..` pops the canonical path, so it leaves a mount point and undoes a
  followed link physically (POSIX); `..` at `/` stays at `/`.
- A name is looked up first among the mount points of the directory
  (folded when the directory's file system folds), then in the file
  system: through the casefold index on a folding backend file system,
  exactly on a case-sensitive one, by `/QSYS.LIB` naming in the store
  view, by name in the device and optical views.
- A symbolic link (not the last component, or the last when following)
  has its text spliced in front of the remaining components; an
  absolute link restarts at `/`. More than `SYMLOOP_MAX` (40) links is
  `ELOOP` (`CPFA0A3`).
- Every directory passed needs `*X` (`Op::Search`); a refusal is
  `EACCES` and an `AF` event.
- A trailing slash requires a directory (`ENOTDIR`).

The **casefold index** maps folded name to stored names for each
backend directory it has seen (built from one `readdir`, at most 4096
directories cached). Every change made through the crate updates it
(create, link, rename, unlink, rmdir drop or add the name, a directory
moved or removed drops its subtree). A miss is checked against the
backend: when the directory's modification time differs from the
index's, or the exact name exists, the index is rebuilt once, so names
made behind the crate's back are found. When two stored names fold
alike (only possible behind the crate's back), the exact name wins,
else the first in byte order; `RCLLNK` reports the pair.

Folding (`path::fold`) maps each character to its uppercase when that
is one character (`ß` stays `ß`), so it is a function of single
characters and never changes a name's length in characters.

### `/QSYS.LIB`

Components are `NAME.TYPE` with the type's name as extension (`*FILE`
is `.FILE`, every type of `vfs::qsys::TYPES`, `*MBR` is `.MBR`); a name
that is not simple is quoted (`"a.b".FILE`), others are uppercased.
Objects of `QSYS` sit directly in `/QSYS.LIB`, libraries as `NAME.LIB`.
`vfs::qsys::node_of` and `path_of` convert both ways
(`Qp0lCvtPathToQSYSObjName`).

A physical file is a directory of members. A member reads and writes
as a byte stream, as IBM i 7.5 does (checked on pub400,
`NET_OPEN_QUESTIONS.md` 8): a source member is its data records
(without sequence number and date, trailing blanks kept) back to back;
opened with `O_TEXTDATA` each record is followed by the CCSID's line
feed (`0x25` in EBCDIC). `stat` counts two bytes of line end per
record, as IBM's `st_size` does. A data member is its fixed-length
records back to back. Written data becomes records again when the
descriptor is closed, synced or its owner released: a text-mode stream
is split at line feeds (a carriage return before one is dropped, each
line padded with blanks or cut to the record length), a binary one at
the record length with the last record padded. Records go in through
`db::source::put_records` (sequence numbers from 1.00, today's date)
and `db::util::put_records`, each allocating the member `*EXCL` as
`CLRPFM` does. `*USRSPC` and `*SAVF` read and write as their bytes (a
save file keeps its length in its first eight bytes until Phase 11
gives it records). The member's CCSID is the file's. Other
objects are `S_IFNATIVE` and do not open (`ENOTSUP`). `mkdir` of
`NAME.LIB` makes a library, `open(O_CREAT)` of `NAME.MBR` adds a
member, `unlink` and `rmdir` delete. The user's authority to an object
is the view's answer (the Phase 6 search) mapped to `*RWX` as below.

## Disk format

The IFS root (`/`, and `/QOpenSys` beneath it) lives on an xfs
filesystem in its own GPT partition. Everything in this section sits
below the path resolver: the partition, the filesystem `xfsfmt` writes
into it, and how `slic` mounts it.

### The partition

| | |
|---|---|
| Name | `QS400-IFS` (also the filesystem label) |
| Type GUID | `90afa14f-9e50-4da1-99ae-515334304946` (Quartz, generated once) |
| GPT entry | 4 (ASP1 keeps entry 3, as on disks laid out before the IFS) |
| On disk | ESP, LIC, **IFS**, ASP1 |
| Size | a quarter of the space after the LIC partition, clamped to 512 MiB .. 256 GiB, in whole 4 KiB |

The 4 GiB development image gets 943.5 MiB; the smallest disk the
layout accepts (`diskimg::MIN_DISK_BYTES`, about 898 MiB) gets 512 MiB.
`diskimg::Plan` lays it out, `diskimg::write_image` formats it (the
built image and the installer both), `diskimg::locate::ifs` finds it,
preferring the one on the same disk as the LIC partition.

### The filesystem

`ifs/xfsfmt` is a pure-Rust mkfs. It writes an empty, clean XFS v5
filesystem onto any `Write + Seek` device:

```rust
let geo = xfsfmt::format(&mut dev, bytes, &xfsfmt::Options::new(uuid, time))?;
let info = xfsfmt::probe(&mut dev)?; // Some(Info { uuid, label, dblocks, .. })
```

| Choice | Value |
|---|---|
| Version | 5 (`versionnum` 0xb4a5, plus SECTORBIT with 4 KiB sectors) |
| Features | CRCs, ftype (incompat), bigtime (incompat), finobt and inobtcount (ro-compat). No rmapbt, reflink, sparse inodes, nrext64, exchange, parent pointers, metadir, quotas, realtime. |
| Block | 4096 bytes; directory blocks the same |
| Inode | 512 bytes, 8 per block, chunks of 64 aligned to 4 blocks |
| Sector | 512 by default, 4096 by option (`Options::sector_size`); the log uses the same |
| AGs | 16 MiB each below 128 MiB; otherwise 4, more once one would pass 1 TiB. All AGs equal; blocks short of a whole AG at the end go unused. |
| Log | internal, in AG `agcount / 2` right after its btree roots: the larger of 1/2048 of the filesystem and 10 MiB (64 MiB at 300 MiB and up), at most 1 GiB. Well above the kernel's computed minimum for this geometry. |
| Root | inode 96: AG 0 block 12, where `xfs_repair` computes mkfs would put it. The chunk also holds the empty realtime bitmap (97, `NEWRTBM`) and summary (98) inodes. |
| Label | `QS400-IFS` |
| UUID | given: `51533430-0000-4000-8000-000000000006` in built images (`Guids::FIXED.ifs_fs`), random at install |

Each AG: superblock, AGF, AGI and AGFL in sectors 0 to 3; then one leaf
each for the free space btrees (by block, by size) and the inode and
free inode btrees; four blocks on the AG free list (slots 1 to 4, as
mkfs leaves them); the rest free. Secondary superblocks are copies of
the primary, as the kernel's own growfs writes them. The root directory
is short form with no entries and its parent itself. Every metadata
block carries the filesystem UUID, its owner and address, LSN 0 and a
CRC-32C.

The log is cleared and holds one cycle-1 record at its first block: an
unmount record (transaction ID `0xb0c0d0d0`, the userspace mark), its
first word packed into the header's cycle data, the record CRC left
zero as mkfs leaves it. The kernel finds the head there and mounts
without recovery.

The order of writes protects a device that held a filesystem before:
the old primary superblock is zeroed first and the new one written
last, after a flush, so an interrupted format never mounts.

Determinism: the output depends only on the size and the options. The
built image passes a fixed UUID, the build's `SOURCE_DATE_EPOCH` as the
root's creation time, and `zeroed` (a new image file reads as zeros, so
the log is not written out and the image stays sparse). The installer
passes a random UUID, the current time, and clears the log.

### Mounting

At IPL `slic` mounts the partition as xfs at `/run/qs400/ifs`
(`nosuid,nodev,noexec`) just before the storage service starts; the
service inherits the mount namespace. A D IPL from the media does not
mount it, since it may repartition the disk. At power down it is
unmounted once the service has ended and the store is closed; the
installer unmounts it before writing a disk. Neither a missing nor an
unmountable IFS stops the IPL.

| Code | Phase | Meaning |
|---|---|---|
| C6001012 | OS | IFS mounted |
| A6001270 | attention | IFS partition missing (IPL continues) |
| A6001275 | attention | IFS mount failed (IPL continues) |
| D1001012 | power down | IFS unmounted |

The service console's `svc.ifs` prints the mount, the label and UUID,
and the capacity and free space from `statvfs`. DST's `Work with
Licensed Internal Code` display shows the partition, its size and
whether it is mounted; IBM's disk configuration displays describe ASP
units, which the IFS partition is not, so it is not shown there.

### Disks laid out before the IFS

RCLSTG (without `*CHECK`) adds the partition to a load source that has
none, when at least 512 MiB is unused after its last partition (a disk
that has grown): the filesystem is written into the unused space first,
then the partition table is rewritten for the disk's present size with
every existing entry unchanged and the IFS in the first free entry from
4. The kernel sees it at the next IPL, which mounts it. `RCLSTG
*CHECK` only reports whether the partition is there.

BLOCKED(phase10): when the disk has no such space, RCLSTG says so and
changes nothing. Making room by shrinking ASP1 would need the store to
give back the tail of its unit (it can truncate a span only when the
tail is free, as mirroring does) and the kernel to resize a partition
that is in use (`BLKPG`, a new unsafe ioctl); neither is in place.

### Kernel

`CONFIG_XFS_FS=y` was already required. Nothing else is: the
filesystem uses no ACLs (`XFS_POSIX_ACL` stays off), no quotas and no
realtime volume, and v5 needs no option beyond what `XFS_FS` selects.

### Verification

- `cargo test -p xfsfmt`: CRC-32C, field offsets against
  `xfs_format.h`/`xfs_log_format.h`, and the superblock, AGF, root inode
  and log record byte for byte against `mkfs.xfs` 6.18 for the same
  geometry and UUID; geometry and determinism tests.
- `tools/conformance/xfs/verify.sh build|check DIR` (Linux, `check` as
  root with xfsprogs): images of 64 MiB, 512 MiB, 1 GiB, 20 GiB (sparse)
  and an odd size, at both sector sizes, must pass `xfs_repair -n`,
  mount read-write with no kernel complaint, take file traffic (a large
  file, 500 small ones, hard and symbolic links, xattrs, renames,
  deletes), unmount, pass `xfs_repair -n` again and remount.
- QEMU step 59 (`59 IFS mount`): C6001012 in the SRC log and `svc.ifs`
  showing a read-write xfs at `/run/qs400/ifs` with plausible capacity.

## Metadata

Everything QS/400 decides about an object lives in one record in the
extended attribute `trusted.qs400.meta`, read and written only by
`ifs::meta`. xfs never decides authority: the mode bits, owner UID and
group GID it holds are a mirror written from the record for tools
(`meta::mirror`), never read for a decision.

The record is `QM`, a version byte (1), then fields as tag (u8),
length (u16 LE) and value; integers little-endian, text UTF-8. Unknown
tags are skipped (later versions add fields); the owner is required.

| Tag | Field | Value |
|---|---|---|
| 1 | owner profile | text |
| 2 | owner authority | u32, `sec::auth::bits` |
| 3 | primary group | text, absent for none |
| 4 | group authority | u32 (kept with no group: a later `CHGPGP` hands it on) |
| 5 | `*PUBLIC` | u32 |
| 6 | `*PUBLIC` from the list | u8 1 |
| 7 | a private authority (repeated) | u32 authority, then the profile |
| 8 | authorization list | text |
| 9 | CCSID | u16 |
| 10 | code page | u16 |
| 11 | flags | u32, below |
| 12 | `*DISKSTGOPT`, `*MAINSTGOPT` | u8 each: 0 `*NORMAL`, 1 `*MINIMIZE`, 2 `*DYNAMIC` |
| 13 | `*CRTOBJSCAN`, `*SCAN` | u8 each: 0 `*NO`, 1 `*YES`, 2 `*CHGONLY` |
| 14 | `OBJAUD` | u8: 0 `*NONE`, 1 `*USRPRF`, 2 `*CHANGE`, 3 `*ALL` |
| 15 | `*CRTOBJAUD` | u8: 0xFF `*SYSVAL`, else as tag 14 |
| 16 | check-out | i64 time, then the user |
| 17 | journaling | i64 start, u8 images, u8 omit open/close, u8 inherit, 10-byte JID, then `LIB/JOURNAL` |
| 18 | created | i64 nanoseconds |
| 19 | generation | u32 (with the inode number, the file ID) |
| 20 | text | text, 50 characters |

| Flag | Attribute |
|---|---|
| 0x0001 | `*HIDDEN` |
| 0x0002 | `*RDONLY` (opens for writing fail `EROOBJ`) |
| 0x0004 | `*SYSTEM` |
| 0x0008 | `*PCARCHIVE` |
| 0x0010 | `*SYSARCHIVE` |
| 0x0020 | `*ALWSAV` |
| 0x0040 | `*ALWCKPWRT` |
| 0x0080 | `*INHALWCKPWRT` (directories) |
| 0x0100 | `*RSTDRNMUNL` (`S_ISVTX`) |
| 0x0200 | `S_ISUID` |
| 0x0400 | `S_ISGID` |
| 0x0800 | a system directory or UDFS root |
| 0x1000 | a UDFS root created `CASE(*MIXED)` |

**Fallback.** A node without a record (a crash between creating it and
writing the record, or a file made outside QS/400) or with one that does
not decode gets a defined record: owner from the mirrored UID (the
profile with it, else `QDFTOWN`), group from the GID, data authorities
from the mode bits, every object authority for the owner, none for
others, CCSID 1208. `RCLLNK` writes it.

## Authority

IFS data authorities map onto the system's bits as IBM documents:

| IFS | Bits |
|---|---|
| `*R` | `*OBJOPR *READ` |
| `*W` | `*OBJOPR *ADD *UPD *DLT` |
| `*X` | `*OBJOPR *EXECUTE` |
| `*RW`, `*RX`, `*WX`, `*RWX` | the unions (`*RWX` is `*CHANGE`, `*RX` is `*USE`) |
| `*EXCLUDE` | none |

Object authorities are `*OBJEXIST *OBJMGT *OBJALTER *OBJREF`. A set
whose data bits are not one of the combinations shows as `USER DEF`.
Mode bits are the owner's, primary group's and `*PUBLIC`'s data
authorities as `rwx` triples (`*PUBLIC` from a list and the group bits
without a group show as none); `chmod` replaces those data authorities
and keeps object authorities.

The search (`auth::check`) is IBM's order, as `sec::auth::search` for
library objects: `*ALLOBJ` (or `QSECURITY` 20); the user as owner,
private authority or list entry (decides, even `*EXCLUDE`); a group's
`*ALLOBJ`; the groups (primary group authority, or the group as owner,
private or list entry; they add up); `*PUBLIC` (or the list's); then
each adopted owner. The `AuthEnv` answers lists, profiles and IDs.

| Operation | Directory | Object |
|---|---|---|
| each directory of a path | | `*X` |
| open read / write / read-write | | `*R` / `*W` / `*RW` (`O_TRUNC` counts as write) |
| create a file, FIFO, socket, link; mkdir | `*WX` | |
| unlink, rmdir | `*WX` | `*OBJEXIST` |
| rename or move | `*WX` on both | `*OBJMGT`; `*OBJEXIST` on a replaced object |
| link | `*WX` | `*OBJEXIST` |
| readdir (`opendir`) | | `*R` |
| stat, readlink, `Qp0lGetAttr` | | |
| chmod, chown, chgrp, `CHGOWN`, `CHGPGP` | | owner or `*ALLOBJ` |
| `CHGAUT` | | owner, `*ALLOBJ` or `*OBJMGT` |
| PC attributes, CCSID, text; `utime` to now | | owner or `*W` |
| storage, save, scan attributes | | owner or `*OBJALTER` |
| auditing value | | `*AUDIT` special authority |
| `utime` with times | | owner or `*ALLOBJ` |
| check-out, truncate by path | | `*W` |
| `STRJRN`, `ENDJRN` | | `*OBJMGT` |
| `CPY` source | | `*R` |
| `SAV` | | `*OBJEXIST` or `*SAVSYS` |

`auth::need::rule_of` is the table; its test lists it row by row. A
directory with `*RSTDRNMUNL` (`/tmp`) lets only the object's owner, the
directory's owner or `*ALLOBJ` unlink or rename in it (`EPERM`). A
`CHGAUT` caller that is neither owner nor `*ALLOBJ` may give only
authorities it has and may not change the list (`EPERM`). `DSPAUT`
shows every row to the owner, `*ALLOBJ` and `*OBJMGT`, else `*PUBLIC`
and the caller's own.

**New objects** (`open(O_CREAT)`, `mkdir`, `symlink`, `mkfifo`): the
caller owns them with every object authority; the primary group,
private authorities, authorization list and the object authorities of
the group and `*PUBLIC` come from the parent directory; data
authorities from the mode less the umask (0o022 by default), or with
`O_INHERITMODE` from the parent's; `OBJAUD` from the parent's
`*CRTOBJAUD` (`*SYSVAL` is `QCRTOBJAUD`); `*SCAN` from its
`*CRTOBJSCAN`; a directory inherits both and the parent's CCSID; a
stream file is tagged with the `O_CCSID`/`O_CODEPAGE` value, else 1208
(`Config::default_ccsid`). Journaling that the parent marks to inherit
is copied.

`CHGOWN` gives the new owner the owner's authority (its private one
goes); the old owner keeps it privately unless `RVKOLDAUT(*YES)`
(`chown()` always revokes). `CHGPGP` takes a private authority the new
group had as its group authority, else the group authority already
there; the old group keeps its authority privately unless revoked.

## Descriptors and locks

Descriptors are per owner, lowest free first, at most
`Config::max_fds` (`EMFILE`); `dup`, `dup2` and `F_DUPFD` share the open
file description (position, flags). A directory opens read-only (for
`openat`, `fchdir`). An unlinked open file stays readable.
`O_APPEND`, `O_SYNC` and `O_DSYNC` are honoured on every write; `fsync`
syncs the backend file or writes a member back.

**Share modes.** None given is `O_SHARE_RDWR`. A new open is refused
(`EBUSY`) when it would read or write what an existing open's share
mode forbids, or forbid what an existing open does. `lock::share` has
the matrix test.

**Byte-range locks** follow POSIX `fcntl`: an owner's locks never
conflict with each other, a new lock replaces the owner's coverage of
its range (splitting what it overlaps), adjacent equal locks merge,
length 0 runs to end of file and beyond, and closing any descriptor of
the object drops the owner's locks on it. The storage service never
waits: `F_SETLKW` answers `EAGAIN` while it would block and the job
retries (as with data queues); a retry that would wait on an owner
already waiting, directly or not, on the caller is `EDEADLK`.

**Check-out** (`CHKOUT`) is persistent, in the record: others may read
but not open for writing, rename or unlink (`EBUSY`); `CHKIN` by the
user who checked out, the owner or `*ALLOBJ`.

`Ifs::object_locks` and `Ifs::owner_locks` list opens and ranges for
`WRKOBJLCK`.

## Text conversion

`O_TEXTDATA` converts data between the file's CCSID and the open's;
line ends are characters like any other. The open's CCSID is
`conversion_id` with `O_CCSID` (a code page with `O_CODEPAGE`; 0 is the
job's), else the job's. A file the open creates is tagged with
`conversion_id` and its data is not converted, unless `O_TEXT_CREAT`
asks for conversion from `text_conversion_id`. Offsets are the file's:
a read moves the position by the file bytes consumed and keeps
converted bytes that did not fit for the next read; a seek drops them.
A character split across calls is carried; one left at close is
written as the substitution character. CCSID 65535 is never converted.

The `ccsid` crate converts 37, 273, 277, 278, 280, 284, 285, 297, 500,
871 (EBCDIC), 819, 850, 1252, 367 (ASCII-based), 1208 (UTF-8), 1200 and
13488 (UTF-16 big-endian). The single-byte tables are CSVs in
`storage/ccsid/cp/`, generated once with Java's charsets (Temurin 25)
with EBCDIC `0x15` as U+0085 as IBM's tables have it, and CP1252's five
undefined bytes as the C1 controls of the same value. An unmappable
character becomes `0x3F` (EBCDIC), `0x1A` (ASCII) or U+FFFD.

## Errors and messages

`Errno` has IBM's numbers (`EACCES` 3401 ...); `errno::linux` maps the
backend's Linux numbers at the boundary. Every value has the message
`CPE` and its number (`CPE3401`); `Errno::command_message` is the
`CPFA0xx` escape an IFS command sends, checked against `QCPFMSG` on the
oracle (`tools/conformance/oracle/ifs/qcpfmsg-ifs.txt`):

| errno | Message |
|---|---|
| `EACCES`, `EPERM` | `CPFA09C` |
| `EBUSY`, `ETXTBSY`, `ELOCKED` | `CPFA09E` |
| `EDAMAGE` | `CPFA09F` |
| `EEXIST` | `CPFA0A0` |
| `EIO` | `CPFA0A1` |
| `EINVAL` | `CPFA0A2` |
| `ELOOP` | `CPFA0A3` |
| `EMFILE` / `ENFILE` | `CPFA0A4` / `CPFA0A5` |
| `EMLINK` | `CPFA0A6` |
| `ENAMETOOLONG` | `CPFA0A7` |
| `ENOENT` | `CPFA0A9` |
| `ENOSPC` | `CPFA0AA` |
| `ENOTEMPTY` | `CPFA0AC` |
| `ENOTSUP` | `CPFA0AD` |
| `EXDEV` | `CPFA0B0` |
| `EROFS`, `EROOBJ` | `CPFA0B1` |
| `ECONVERT` | `CPFA0D0` |
| `EISDIR` | `CPFA0DA` |
| anything else | `CPFA0D4` with the number |

The `strerror` texts are Quartz's own. Open pairings are in
`NET_OPEN_QUESTIONS.md`.

## Auditing

The caller implements `ops::audit::Audit`. The IFS reports, with the
path, object type and file ID: `ZR`/`ZC` for reads and changes of
objects whose `OBJAUD` asks (`*ALL` both, `*CHANGE` changes, `*USRPRF`
as the user's `OBJAUD`), and always `AF` (authority failure, violation
`A`), `CO`, `DO`, `OM` (detail: the new path), `CA`, `OW`, `PG`, `AD`.
The storage service filters by `QAUDCTL`/`QAUDLVL` and writes
`sec::audit` entries.

## User-defined file systems

`CRTUDFS '/dev/QASPnn/NAME.udfs'` makes the backend directory
`UDFS/QASPnn/NAME.udfs` with a record (owner the caller, `*PUBLIC
*RWX`, `CASE(*MONO)` folding or `CASE(*MIXED)` exact, `*CRTOBJAUD`,
text). `MOUNT` (needs `*IOSYSCFG`) mounts it over an existing directory
of a backend file system that is not a mount point; `UNMOUNT` (by mount
point or UDFS) refuses while anything in it is open, a current
directory, or mounted over; `DLTUDFS` refuses while mounted. `DSPUDFS`
shows the attributes and where it is mounted.

## Copy, move, walk, file IDs

`CPY` goes through the ordinary operations, so it crosses file systems
and checks authority as any caller; the copy gets the source's
authorities and attributes (not its check-out or journaling), owned by
the caller (`OWNER(*KEEP)`: the source's owner, for that owner or
`*ALLOBJ`); `to_ccsid` converts the data (`DTAFMT(*TEXT)`). `MOV` is a
rename within a file system, else a copy of the subtree and its
removal. `Ifs::walk` visits a subtree parents first in name order
(`Qp0lProcessSubtree`); following links never enters a directory twice.
A file ID is the file system, the inode number and the record's
generation; `path_from_file_id` searches that file system.

## `RCLLNK`

`meta::reclaim::run` walks the whole backend: a missing or undecodable
record gets the fallback; an owner that no longer exists becomes
`QDFTOWN`; a missing group, deleted profiles' private authorities, a
missing authorization list and a deleted user's check-out are removed;
stray `trusted.qs400.*` attributes are removed; mirrors that disagree
are rewritten. Link counts that disagree with the links found and names
that fold alike are reported. `check_only` reports without changing.

## Save

`Ifs::save` walks a subtree for `SAV` and hands a `SaveSink` one
`save::Descriptor` per link (skipping `*ALWSAV(*NO)` and the view file
systems), each stream file's data after its descriptor; a second link
to an inode already saved names the first path instead of repeating
the data. A descriptor is `QSVD`, version 1, the path and type
(u32-length text), the metadata record (u32 length), access and
modification times and the data size (i64, i64, u64 LE), and the link
target and hard-link source (a presence byte, then u32-length text).
Phase 11 writes them to media and restores from them.

## In the storage service

`lic/slic/src/stgsvc/ifs.rs` builds the one `Ifs<LinuxBackend>` when
the service starts, if `/run/qs400/ifs` is a mount point
(`/proc/self/mountinfo`), and runs `ensure_layout` (the standard
directories owned by `QSYS`, `*PUBLIC *RX`, `/tmp` and `/QOpenSys/tmp`
`*RWX` with `RSTDRNMUNL`). Otherwise every IFS request answers
`ENOTSUP` (`CPE3440`) through `ifs::client::Unavailable`, and the
service's start line says why. The store side is the crate
`ifs/qsyslib`:

| Part | Does |
|---|---|
| `StoreQsys` | `QsysView` over the store: libraries, objects, members, `*USRSPC`, `*SAVF`, authority by the Phase 6 search (adopted authority included), library and member creation, deletion |
| `StoreEnv` | `AuthEnv`: profile names and `UID`/`GID` (read once per request), groups, authorization lists, `QCRTOBJAUD` |
| `JournalAudit` | `Audit`: each event as a `sec::audit::Event` with the IFS fields (`docs/SECURITY.md` §7), recorded for the request |
| `StoreAccess`, `MemAccess` | one transaction per call; the service's `Lent` reaches the store lent for the request, tests use a store in memory |

A request runs with the store lent and outside the store's guard: the
IFS checks authority itself, through the view's answer, so a refusal is
one `AF` entry, not two. After the answer the service's audit flush
filters the recorded events by `QAUDCTL`/`QAUDLVL` and writes them to
`QAUDJRN`, as for every other request.

### The protocol

One request, `ifs` (tag 440 in `stg::proto`), carries a
`stg::ifs::IfsOp` with one variant per `IfsClient` method (variant tags
0 to 63: open 0, openat 1, close 2, read 3, pread 4, write 5, pwrite
6, lseek 7, fsync 8, ftruncate 9, fstat 10, stat/lstat 11, access 12,
chmod 13, chown 14, link 15, unlink 16, rename 17, symlink 18,
readlink 19, mkdir 20, mkfifo 21, rmdir 22, opendir 23, readdir 24,
rewinddir 25, closedir 26, the whole directory 27, chdir 28, fchdir
29, getcwd 30, dup 31, dup2 32, fcntl 33, byte-range locks 34, the
descriptor's path 35, utime 36, umask 37, pathconf 38, statvfs 39,
get attributes 40, set an attribute 41, DSPAUT 42, CHGAUT 43, revoke
44, CHGOWN 45, CHGPGP 46, CHKOUT 47, CHKIN 48, STRJRN 49, ENDJRN 50,
CPY 51, MOV 52, remove a tree 53, walk 54, path from file ID 55,
CRTUDFS 56, DLTUDFS 57, MOUNT 58, UNMOUNT 59, DSPUDFS 60, RCLLNK 61,
an object's locks 62, the job's locks 63). The answer is an
`IfsReply` whose variant is the method's return type, each a `Result`
whose error is the IBM `errno` number; `Errno::message_id` gives the
`CPExxxx` and `command_message` the `CPFA0xx` a command sends. A reply
of the wrong shape is `EUNKNOWN`. `stg::ifs::serve` maps an operation
to any `IfsClient`, so the service, the tests and a future in-process
caller share it. Reads and writes move at most 1 MiB
(`ifs::client::CHUNK`) per call; `read_all`/`write_all` loop. A
directory stream is read in batches (`readdir` with a count).

The job side is `ifs::client::IfsClient`: `stg::remote::RemoteIfs`
over the job's link (in `stg::remote::Sources`), `LocalIfs` over an
`Ifs` in the process for tests. A job's owner is its job handle; when
its connection ends, `end_job` calls `release_owner` with the store
lent, so open members are written back before the job's table entry
goes. The console's lines use an owner of their own, released after
each line.

### Console and status displays

`svc.ifs` alone reports the mount and capacity; `svc.ifs ls [PATH]`,
`cat PATH` (text mode into UTF-8), `put PATH TEXT` (a new or replaced
stream file tagged 1208), `mkdir`, `rm` (a file or an empty
directory), `stat` (attributes and `DSPAUT`'s rows) and `df [PATH]`
run through the same `Ifs` as the job whose line it is.

IBM keeps the IFS in the system ASP, so its capacity shows where the
ASP's does. `WRKSYSSTS` (the console form, the panel and `QWCRSSTS`)
counts the IFS partition's size and use in `System ASP`, `% system ASP
used` and `Total`. `WRKDSKSTS` adds them to unit 1's `Size` and `%
Used`: the partition is on the load source, which IBM's unit 1 is. No
row is invented for it; IBM's screen lists disk units only.

## Not yet

- Opening a FIFO through the service (it would have to wait) is
  `ENOTSUP`; `mkfifo` and `stat` work.
- `STRJRN` on IFS objects stores the journaling attributes and writes
  no entries.
- Scanning attributes are stored; there are no scan exit points.
- The service holds one lock for the whole IFS state; I/O runs under it.
