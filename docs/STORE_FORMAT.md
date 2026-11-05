# QS/400 object store: on-disk format

Format version 2 (version 1 stores are not opened; V1R0M0 test installs
are reinstalled). This document is written before the code that
implements each structure, and the code's constants are checked against
the tables here by `storage/store/tests/spec.rs`. Every table under a heading of
the form `### \`Name\` (N bytes)` is parsed by that test: the heading name
must match a `Layout` in the code, and the `Offset`, `Size` and `Field`
columns must match its fields exactly. Prose and `Notes` are free.

## 0. Conventions

- A page is 4096 bytes. A unit is addressed in pages. An ASP is the
  concatenation of its spans into one logical page space, where a span
  is one unprotected unit, one mirrored pair of units holding the same
  pages, or one member of a device parity set (§11). `PageNo` is a logical page number in that space (`u64`).
- Multi-byte fields inside page structures are little-endian.
- B+tree keys are big-endian so that byte order equals key order.
- CRC32C (Castagnoli) everywhere.
- Strings are CP037 unless a field says otherwise.

## 1. Pages

Every page has a 4064-byte body followed by a 32-byte trailer. The CRC
covers bytes 4..4096: the body and the rest of the trailer, so a page
whose type, owner or generation is corrupted is caught too. A checksum
mismatch is a hard error that names the page number and the owning
object; nothing silently reads a bad page.

### `PageTrailer` (32 bytes)

At byte offset 4064 of the page.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | crc | CRC32C of page bytes 4..4096 |
| 4 | 2 | page_type | see the table below |
| 6 | 2 | flags | reserved, 0 |
| 8 | 16 | owner | `ObjectId` of the owning object, all zero for metadata |
| 24 | 8 | generation | transaction ID that wrote the page |

Page types:

| Value | Type |
|---|---|
| 0x0000 | free (never valid on read) |
| 0x0001 | unit header |
| 0x0002 | superblock |
| 0x0003 | WAL page |
| 0x0011 | free-log page (§7) |
| 0x0012 | IFS map page (§12) |
| 0x0010 | B+tree node (leaf or internal, see the node header) |
| 0x0020 | object header |
| 0x0021 | object data |
| 0x0022 | extent overflow |
| 0x0023 | index entry overflow |

## 2. Object ID

128 bits. The serial is monotonic per ASP and never reused; the
generation is bumped when an object is deleted so a stale reference can
be detected (MCH3402). The encoding is big-endian in this field order so
the byte order of an encoded ID is also its sort order (ASP, then type,
then serial, then generation).

### `ObjectId` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | asp | ASP number, 0 for the temporary ASP |
| 2 | 2 | type_code | from the type registry |
| 4 | 8 | serial | monotonic per ASP |
| 12 | 4 | generation | bumped on delete |

The all-zero ID means "no object" and marks metadata pages.

## 3. B+tree nodes

One page per node, page type 0x0010. Keys are fixed size per tree
(at most 2048 bytes, so two entries fit a node at the limit); values are
variable (at most 1024 bytes). A tree may fix its key size at run time (file access paths do). Nodes are
never modified in place: a change rewrites the node to a fresh page, and
its parent, up to the root, whose new page number is published through
the transaction. There are no sibling pointers; range scans keep a stack
of ancestors.

### `NodeHeader` (16 bytes)

At offset 0 of the body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | level | 0 leaf, 1 internal |
| 1 | 1 | flags | reserved, 0 |
| 2 | 2 | count | number of keys |
| 4 | 2 | key_len | bytes per key, same for every node of a tree |
| 6 | 2 | reserved | 0 |
| 8 | 8 | reserved2 | 0 |

**Internal node.** Offset 16 holds `first_child` (8 bytes, `PageNo`),
then `count` entries of key followed by child:

### `InternalEntry` (variable)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | key_len | key | lowest key reachable through `child` |
| key_len | 8 | child | `PageNo` |

A key `k` descends into `first_child` when `k < key[0]`, otherwise into
the `child` of the last entry whose key is `<= k`.

**Leaf node.** Offset 16 holds `count` slots. Values are stored at the
end of the body, packed downwards from offset 4064; a slot points at its
value. Slots and values are in key order.

### `LeafSlot` (variable)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | key_len | key | |
| key_len | 2 | value_off | body offset of the value |
| key_len + 2 | 2 | value_len | 0..1024 |

A node fits when its slots or entries and its values occupy at most the
body. On insert a node that no longer fits is split into two halves of
roughly equal byte size; the separator pushed up is the right half's
first key (leaf) or the middle key (internal). On delete a node using
less than a quarter of the body is merged with a sibling when the merged
node fits, otherwise the pair is rebalanced into two nodes.

Bulk load fills leaves to about 90% and builds the internal levels from
the leaves' first keys; input must be strictly ascending.

## 4. Unit header

Every unit begins with a unit header at page 0 and carries a backup at
its last page. Both are ordinary pages (type 0x0001, owner zero) so the
trailer CRC covers them. Neither page is ever allocated. The header is
the unit's own: the halves of a mirrored pair have different headers
(their own UUID and unit number, each naming the other in `mirror_of`)
and are never copied over each other.

### `UnitHeader` (80 bytes)

At offset 0 of the body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400ASP` |
| 8 | 4 | format_version | 2 |
| 12 | 4 | flags | reserved, 0 |
| 16 | 16 | unit_uuid | |
| 32 | 16 | asp_uuid | |
| 48 | 2 | asp_number | 1 for SYSBAS |
| 50 | 2 | unit_number | position in the unit table, from 0; the table wins when they differ |
| 52 | 4 | reserved | 0 |
| 56 | 8 | pages | unit size in pages (the device size) |
| 64 | 16 | mirror_of | unit UUID of the other half of the pair, zero when unprotected |

### Unit states

The unit table (§5) records a state per unit, with the names the disk
configuration displays use:

| Value | State | Meaning |
|---|---|---|
| 0 | Unprotected | not mirrored; the only copy of its span |
| 1 | Active | half of a pair, in step with its partner; reads and writes |
| 2 | Suspended | half of a pair, not written; its data is stale |
| 3 | Resuming | half of a pair being copied from its partner; written, not read |
| 4 | Not operational | attached but failing I/O; treated as suspended |
| 5 | Missing | not found at IPL; treated as suspended |

Every write to a span goes to each half that is Active or Resuming and
`flush` waits for all of them. Reads alternate between Active halves; a
page that fails verification on one half is read from the other and,
when that copy verifies, rewritten on the first. IFS volume pages (§12)
are not verified: only a read that fails goes to the other half. A half that fails I/O
while the other half works becomes Not operational and the pair goes on
with one half.

At open, a pair with one half absent runs on the present half if that
half is Active, and the absent half is recorded Missing. A half found
again after being Missing or Not operational is Suspended: its data is
stale until a resume copies every allocated page of the span onto it,
after which it is Active again. An absent unprotected unit, both halves
absent, or only a stale half present stops the open (disk unit missing).

The resume copies the span in chunks of 256 pages in ascending order.
Which chunks are done is a bitmap the caller persists outside the
store (the LIC partition keeps it, see docs/PHASE1.md) so that a crash
in the middle continues where it stopped. Writes during a resume go to
both halves, so a chunk once copied stays current.

## 5. Superblock

Two copies per unit, at pages 1 and 2. A checkpoint with generation `g`
writes copy `g mod 2` on every attached unit, the halves of a pair and
suspended halves included, so a torn write can only damage the copy
that was already stale and every unit carries the same table. On open
the newest copy whose page verifies, across all units, wins. Opening a
store whose creating release differs in V or R from the running build
is refused (CPF3294); M may differ. The one exception is an older
release the running one may upgrade (`release::store_may_open`, the
upgrade rule of docs/INSTALL.md): the store opens, and the operating
system install records the running release in the superblock once the
upgrade is done (`Store::asp_superblock_set_release`), so the field is
the release that created or last upgraded the ASP.

Logical page numbers: the first span is pages `0..usable_0`, the next
continues at `usable_0`, and so on (`start` in the unit table; both
halves of a pair have the same `start` and `usable_pages`). Pages 0, 1,
2 and the last page of every span are reserved; the WAL ring is
`wal_pages` pages of span 0 from page `wal_start` (3). A unit's own
backup header sits at its physical last page, which for the larger half
of a pair is beyond the span.

### `Superblock` (256 bytes)

At offset 0 of the body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400SUP` |
| 8 | 4 | format_version | 2 |
| 12 | 4 | flags | bit 0 `needs_recovery`, bit 1 `rclstg_required`, bit 2 `ifs_formatted` (§12) |
| 16 | 8 | generation | checkpoint number; newest valid copy wins |
| 24 | 16 | asp_uuid | |
| 40 | 2 | asp_number | |
| 42 | 2 | unit_count | 1..47 |
| 44 | 1 | release_v | creating release |
| 45 | 1 | release_r | |
| 46 | 1 | release_m | |
| 47 | 1 | protection | bit 0 at least one mirrored pair, bit 1 at least one parity set |
| 48 | 8 | alloc_root | free-space tree root, 0 for empty |
| 56 | 8 | free_log_head | first free-log page, 0 for none |
| 64 | 8 | objdir_root | object directory tree root, 0 for empty |
| 72 | 8 | next_serial | next object serial |
| 80 | 8 | wal_start | first page of the WAL ring (unit 0) |
| 88 | 8 | wal_pages | ring length |
| 96 | 8 | wal_head | see §6 |
| 104 | 8 | wal_tail | see §6 |
| 112 | 8 | last_txn | newest committed transaction at this checkpoint |
| 120 | 8 | install_time | Unix seconds |
| 128 | 8 | last_rclstg_time | Unix seconds, 0 for never |
| 136 | 16 | qsys | `ObjectId` of QSYS, zero before the object layer initializes |
| 152 | 16 | sysval | `ObjectId` of the `*SYSVAL` object, zero before it exists |
| 168 | 1 | parity_work_set | parity set whose synchronization or rebuild is under way, 0 for none (§11) |
| 169 | 1 | parity_work_kind | 1 synchronization, 2 rebuild |
| 170 | 1 | parity_work_member | member being rebuilt; 0 for a synchronization |
| 171 | 5 | reserved3 | 0 |
| 176 | 8 | parity_work_done | blocks of the set done, as of this checkpoint |
| 184 | 8 | ifs_map | first IFS map page (§12), 0 when the IFS is not in the ASP |
| 192 | 8 | ifs_pages | pages of the IFS volume: the sum of its extents |
| 200 | 56 | reserved2 | 0 |

The unit table follows at body offset 256: `unit_count` entries, one
per unit in unit-number order. Units are added, removed and paired only
through the service tools; the table never changes on its own.

Only the unit backing the last span can leave the configuration; halves
paired later may follow it in the table and move down one number. Its
pages move to other spans and are checkpointed first; then the table
without it is checkpointed; only then is its header wiped. A crash
before that checkpoint leaves the old table, which opens: a unit header
whose `unit_number` was already rewritten is matched by UUID and
corrected at open. A crash after it leaves a device with a stale header
of this ASP, not in the table, which is ignored. Starting mirrored
protection on units that hold data is this removal once per partner,
each followed by forming the pair, so every state in between is a valid
configuration.

### `UnitEntry` (80 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | unit_uuid | must match the unit header |
| 16 | 8 | pages | unit size in pages (the device size) |
| 24 | 8 | start | logical page number of the span's page 0 |
| 32 | 8 | usable_pages | pages of the span: `pages`, or the smaller of a pair |
| 40 | 2 | mirror_partner | unit number of the other half, 0xFFFF when unprotected |
| 42 | 1 | state | see §4, unit states |
| 43 | 1 | parity_set | device parity set, 1..15; 0 when the unit is in none (§11) |
| 44 | 1 | parity_member | position in the set, from 0 |
| 45 | 1 | parity_level | 5 RAID 5, 6 RAID 6; 0 when in no set |
| 46 | 1 | parity_state | see §11, parity unit states |
| 47 | 1 | reserved | 0 |
| 48 | 32 | serial | the device's serial number, ASCII, zero padded; blank means use the unit UUID |

## 6. WAL

A ring of `wal_pages` pages on unit 0 from `wal_start`, reserved at
format. Every transaction appends one record and flushes; the record is
committed when its last page is intact. Pages are numbered by a sequence
`seq` that never wraps; page `seq` lives at ring index `seq mod
wal_pages`. `wal_tail` in the superblock is the sequence of the first
page recovery reads; `wal_head` is the next sequence to write at the
time of the checkpoint.

A record is `total_len` payload bytes split over consecutive pages,
4032 bytes each. Commit order: the transaction's copy-on-write pages
and every record page but the last are written and every unit flushed;
then the last page is written and unit 0 flushed. So a valid last page
(page CRC, `seq`, `last` flag, `record_crc` over the payload) proves
the whole transaction is durable.

A group commit (`Store::begin_group` to `end_group`) merges the records
of the transactions committed in it into one record: the allocations,
frees, roots, images and patches of each, in commit order, under the
last transaction's ID. Their copy-on-write pages are written at once,
their in-place pages only after the merged record is durable; until
then the store serves them from memory. A checkpoint, or a record too
large to join, logs the group first. So recovery finds the whole group
or none of it, and a group costs the flushes of one record. The storage
service groups the ends of the jobs it finds gone, which answer no one.

Recovery reads from `wal_tail`: a page whose CRC fails or whose `seq`
is not the expected one (a page from an earlier lap of the ring, or
never written) ends the log, as does a record with no valid last page.
Every complete record before that point is replayed in order, then a
checkpoint is taken.

### `WalPageHeader` (32 bytes)

At offset 0 of the body; payload from offset 32.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | seq | page sequence number |
| 8 | 8 | txn | transaction ID |
| 16 | 4 | total_len | payload bytes of the record |
| 20 | 4 | offset | this page's position in the payload |
| 24 | 2 | len | payload bytes in this page, at most 4032 |
| 26 | 2 | flags | bit 0 first page, bit 1 last page |
| 28 | 4 | record_crc | CRC32C of the whole payload, checked on the last page |

### `WalRecord` (32 bytes)

At offset 0 of the payload, followed by the entries in this order:
`root_count` roots, `alloc_count` allocations and `free_count` frees
(both in the `FreeLogEntry` layout), `image_count` page images and
`patch_count` patches.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | txn | repeated |
| 8 | 8 | next_serial | object serial counter after this transaction |
| 16 | 2 | root_count | |
| 18 | 2 | alloc_count | |
| 20 | 2 | free_count | |
| 22 | 2 | image_count | |
| 24 | 4 | patch_count | |
| 28 | 4 | reserved | 0 |

### `RootEntry` (24 bytes)

A tree root published by the transaction. Owner zero is the object
directory; a library's directory root is published through its header
page instead (a patch).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | owner | `ObjectId` |
| 16 | 8 | root | page, 0 for empty |

### `ImageEntry` (4104 bytes)

The whole page as written, for the first in-place change of a page
since the last checkpoint. Recovery writes it back as is, so a torn
in-place write is repaired.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | page | |
| 8 | 4096 | image | the page, trailer included |

### `PatchEntry` (variable)

An in-place change to a page already imaged since the last checkpoint.
Recovery applies the bytes to the body and reseals the page.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | page | |
| 8 | 2 | offset | body offset |
| 10 | 2 | len | bytes |
| 12 | len | bytes | |

In-place changes are for the objects where rewriting a page per update
is wrong (data areas, data queues, user spaces). A page is imaged on its
first in-place change after a checkpoint, then patched.

## 7. Free space

A page is free when it lies in a run of the free-space tree or of the
free log, and was not allocated by a transaction committed since the
last checkpoint (those allocations are in the WAL, §6). The tree is a
B+tree (§3) keyed by unit and page, valued by run length, written fresh
at every checkpoint; between checkpoints the in-memory bitmap is the
truth.

### `FreeKey` (10 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | unit | big-endian |
| 2 | 8 | lba | first free page, unit-relative, big-endian |

### `FreeLen` (8 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | len | run length in pages |

Runs in the tree never touch: a run that would be adjacent to another is
merged into it.

**Free log.** A checkpoint writes the tree to pages taken from the free
runs in one or more chunks; the unused tail of each chunk cannot be in
the tree (the tree was written before the tail was known), so the
checkpoint lists the tails in one free-log page (type 0x0011). On open
the log's runs are free, like the tree's. The next checkpoint lists the
old tree and log pages as free again.

### `FreeLogHeader` (16 bytes)

At offset 0 of the body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | next | next log page, 0 for the last |
| 8 | 2 | count | entries in this page, at most 168 |
| 10 | 6 | reserved | 0 |

### `FreeLogEntry` (24 bytes)

From body offset 16, `count` entries.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | unit | |
| 2 | 6 | reserved | 0 |
| 8 | 8 | lba | unit-relative |
| 16 | 8 | len | pages |

**Checkpoint.** The new tree is bulk-loaded from the set that will be
free once the checkpoint is durable: the free runs now, the runs freed
since the last checkpoint, and the pages of the previous tree and log.
The tree's own pages come from chunks taken from the free runs first
and removed from that set; the number of pages a bulk load needs is
known from the entry count, so the chunks are sized exactly (one page
over, for the log). Everything the checkpoint writes lands on pages the
previous superblock considers free, so a crash at any point leaves the
previous checkpoint intact. The allocator keeps 1/64 of the ASP (at
least 64 pages, at most 16384) back from ordinary allocation so a
checkpoint on a full ASP can still write its tree.

## 8. Objects

Every object has one header page (type 0x0020, owner the object's own
ID) whose location the object directory gives. The header's fixed part
is 1024 bytes; the type-specific attribute area is the 1024 bytes from
body offset 1024; the rest of the body is reserved for later phases
(database, journaling, more authority).

Object IDs come from the ASP's serial counter and are never reused. A
lookup of a serial below the counter that the directory no longer holds
is a stale reference (MCH3402); a serial at or above it was never an
object.

### `ObjectHeader` (1024 bytes)

At offset 0 of the body. Text fields are CP037, space padded.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400OBJ` |
| 8 | 4 | header_version | 2; a version 1 header (before `sql`) reads as having no SQL information and is written as version 2 at its next change |
| 12 | 4 | flags | bit 0: a program that adopts its owner's authority (`USRPRF(*OWNER)`) |
| 16 | 16 | id | `ObjectId` |
| 32 | 16 | library | `ObjectId` of the containing library; QSYS contains itself |
| 48 | 10 | name | |
| 58 | 10 | attribute | `PF`, `RPGLE`, ... or blank |
| 68 | 50 | text | description |
| 118 | 10 | owner | user profile name |
| 128 | 10 | primary_group | blank for none |
| 138 | 10 | creator | |
| 148 | 8 | system | created-on system name |
| 156 | 1 | release_v | creating release |
| 157 | 1 | release_r | |
| 158 | 1 | release_m | |
| 159 | 1 | reserved2 | 0 |
| 160 | 8 | created | Unix seconds |
| 168 | 8 | changed | |
| 176 | 8 | last_saved | 0 for never |
| 184 | 8 | last_restored | |
| 192 | 8 | last_used | date, Unix seconds at midnight UTC |
| 200 | 8 | reset_date | |
| 208 | 4 | days_used | |
| 212 | 4 | public_authority | bits, see below |
| 216 | 10 | autl | authorization list name, blank for none |
| 226 | 2 | private_count | entries used in `private_auths` |
| 228 | 256 | private_auths | 16 × `PrivateAuth` |
| 484 | 8 | private_overflow | root of a B+tree of more entries (key profile, value bits), 0 for none |
| 492 | 8 | size_pages | pages in the extents |
| 500 | 2 | extent_count | entries used in `extents` |
| 502 | 1 | domain | 0 `*SYSTEM`, 1 `*USER` |
| 503 | 1 | state | 0 system, 1 user |
| 504 | 1 | damaged | |
| 505 | 1 | temporary | |
| 506 | 1 | compression | always 0 |
| 507 | 1 | reserved3 | 0 |
| 508 | 256 | extents | 16 × `ExtentRef` |
| 764 | 8 | extent_overflow | page of more extents, 0 for none |
| 772 | 10 | audit | `OBJAUD`: `*NONE`, `*USRPRF`, `*CHANGE`, `*ALL`; blank is `*NONE` |
| 782 | 2 | reserved4 | 0 |
| 784 | 32 | tree_roots | 4 × page: roots of B+trees the payload owns, 0 for none; RCLSTG walks them, delete frees them |
| 816 | 4 | auth_flags | bit 0: the owner's authority was reduced from `*ALL` |
| 820 | 4 | owner_authority | bits, meaningful when bit 0 of `auth_flags` is set |
| 824 | 4 | group_authority | the primary group's bits |
| 828 | 4 | reserved5 | 0 |
| 832 | 8 | auth_gen | authority generation, bumped by every authority change |
| 840 | 8 | save_size | bytes the object took in its last save, 0 for never saved |
| 848 | 10 | save_command | `SAVLIB`, `SAVOBJ`, ...; zero for never saved |
| 858 | 10 | save_device | device name, or `*SAVF` |
| 868 | 10 | save_file | the save file, when the device was `*SAVF` |
| 878 | 10 | save_file_lib | its library |
| 888 | 6 | save_volume | first volume of the save |
| 894 | 17 | save_label | tape or optical file label |
| 911 | 1 | reserved6 | 0 |
| 912 | 4 | save_seq | tape file sequence number |
| 916 | 64 | signature | ed25519 signature over the object's content digest (`docs/SAVE_FORMAT.md` §5) |
| 980 | 8 | sig_key | first 8 bytes of the SHA-256 of the signer's public key |
| 988 | 4 | sig_flags | bit 0 signed, bit 1 signed by a key the system trusts for system objects |
| 992 | 8 | change | the ID of the last transaction that changed the object; 0 for none yet |
| 1000 | 16 | sql | `ObjectId` of the owned `*SQLINF` holding the object's SQL attributes (`docs/DB_FORMAT.md` §8); zero for none |
| 1016 | 8 | reserved7 | 0 |

Public authority bits: 0x0001 `*OBJOPR`, 0x0002 `*OBJMGT`, 0x0004
`*OBJEXIST`, 0x0008 `*OBJALTER`, 0x0010 `*OBJREF`, 0x0020 `*READ`,
0x0040 `*ADD`, 0x0080 `*UPD`, 0x0100 `*DLT`, 0x0200 `*EXECUTE`, 0x0400 `*AUTLMGT`; 0 is
`*EXCLUDE`; 0x8000_0000 means the public authority comes from the
authorization list. Phase 6 enforces them; `docs/SECURITY.md` has the
search order. The private authority overflow tree's pages are B+tree
nodes owned by the object; RCLSTG walks it and delete frees it.

`change` is the object's change stamp. A transaction that writes,
patches or publishes a root for any page owned by an object (its
header, attribute area, extents, trees, records) sets, at commit, the
`change` of each such object still in the directory to its own
transaction ID, in the same WAL record. Transaction IDs rise and are
never reused for a committed transaction (the superblock's `last_txn`
and the WAL carry them over a restart), so the stamp rises on every
committed change and never repeats; changes in one transaction are one
change. A header written before the stamp existed reads 0 and is
stamped at its next change. The IFS answers it as `/QSYS.LIB`'s change
counter (`docs/IFS.md`).

### `PrivateAuth` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | profile | user profile name |
| 10 | 4 | bits | as public authority |
| 14 | 2 | reserved | 0 |

### `ExtentRef` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | start | logical page |
| 8 | 8 | len | pages |

### `ExtentOverflowHeader` (16 bytes)

At offset 0 of an extent overflow page (type 0x0022), followed by
`count` × `ExtentRef`; at most 253 per page. Phase 2 uses one overflow
page per object.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | count | |
| 2 | 14 | reserved | 0 |

### `LibAttr` (32 bytes)

The attribute area of a `*LIB` object (body offset 1024).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | dir_root | directory tree root, 0 for empty |
| 8 | 1 | lib_type | 0 `*PROD`, 1 `*TEST` |
| 9 | 1 | reserved | 0 |
| 10 | 10 | create_authority | `*SYSVAL`, `*CHANGE`, `*ALL`, `*USE`, `*EXCLUDE` |
| 20 | 2 | asp_number | |
| 22 | 2 | reserved2 | 0 |
| 24 | 8 | object_count | entries in the directory |

**Object directory.** A B+tree per ASP keyed by `ObjectId` (its 16-byte
encoding) with the header page number as the value; the root is in the
superblock and is published by the WAL root entry with owner zero.

**Library directory.** A B+tree per library, owned by the library,
keyed by type code and name, valued by `ObjectId`. Its root lives in
the library's attribute area and is updated in place. Because names
are CP037 and keys sort byte-wise, a scan of one type lists objects in
the order a display would.

### `DirKey` (12 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | type_code | big-endian |
| 2 | 10 | name | CP037 |

## 9. Type registry

Type codes are fixed by `storage/store/src/obj/types.rs` and appear in IDs,
directory keys and headers; see that table for the list. `*SYSVAL`
(0x00FF) is internal: one instance, in QSYS, not user-creatable.

`*MBR` (0x00FE) is internal too: a database file member, created only by
the database layer and owned by its `*FILE`.

`*SQLINF` (0x00FD) is internal: the SQL attributes of one object (its
SQL name, definition text, constraints, triggers, routine, sequence or
variable state; `docs/DB_FORMAT.md` §8), owned by that object and named
by its header's `sql` field. Deleting the owner deletes it. A library's
SQL names are not kept this way: its header's tree root slot 0 is the
library's SQL name index (`docs/DB_FORMAT.md` §8.2).

**Owned objects.** An object whose `library` field names an object that
is not a library belongs to that object and has no library directory
entry; it is reached through its owner and listed only in the object
directory. RCLSTG keeps it while the owner exists and removes it, pages
and trees included, when the owner is gone.

## 10. Payload types

An object's payload lives in its extents. For the types below the
extents form one byte space: page `i` of the concatenated extents holds
bytes `i * 4064 ..` of the space (the body of a data page, type
0x0021, owned by the object). A change to part of a page is an in-place
change logged as a WAL image or patch (§6). A user space, data queue
or system value write covering at least 4 whole pages puts them on
fresh pages (written before the WAL record,
like any new page) that take the old pages' place in the extent list;
the old pages are freed and the header, and its overflow page, are
rewritten in the same transaction. Adjacent runs merge; when the list
would outgrow its 269 slots the write is patched in place instead.

### `DtaaraAttr` (16 bytes)

The value: `*CHAR` the `length` CP037 bytes; `*DEC` a 16-byte two's
complement little-endian integer scaled by `10^decimals`; `*LGL` one
byte, 0 or 1. A value of at most 1008 bytes lives in the attribute area
from offset 16 (`inline` is 1) and the object has no payload pages; a
longer `*CHAR` value is the first bytes of the byte space.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | kind | 0 `*CHAR`, 1 `*DEC`, 2 `*LGL` |
| 1 | 1 | decimals | `*DEC` only, 0..9 |
| 2 | 2 | length | `*CHAR` 1..2000, `*DEC` digits 1..24, `*LGL` 1 |
| 4 | 1 | locked | lock flag for a later phase |
| 5 | 1 | inline | 1 when the value is in the attribute area |
| 6 | 2 | reserved | 0 |
| 8 | 8 | reserved2 | 0 |

### `DtaqAttr` (48 bytes)

Entries are appended at `tail`; `head` is the first live entry. A
receive marks an entry dead; when the queue empties both offsets reset
to 0, which is how space is reclaimed. `*USRQ` is a `*DTAQ` without a
sender ID.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | order | 0 `*FIFO`, 1 `*LIFO`, 2 `*KEYED` |
| 1 | 1 | sender_id | 1 when entries carry a sender |
| 2 | 2 | key_len | 0 unless keyed, else 1..256 |
| 4 | 4 | max_entry_len | 1..64512 |
| 8 | 8 | head | byte offset of the first live entry |
| 16 | 8 | tail | byte offset for the next entry |
| 24 | 8 | entry_count | live entries |
| 32 | 8 | next_seq | sequence of the next entry |
| 40 | 8 | space_bytes | bytes allocated to the byte space |

### `DtaqEntry` (48 bytes)

Followed by `key_len` key bytes, `len` data bytes, and padding to a
multiple of 8.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | seq | |
| 8 | 4 | len | data bytes |
| 12 | 2 | key_len | |
| 14 | 2 | flags | bit 0 live |
| 16 | 26 | sender | job name, user, number (10, 10, 6), CP037; blank without sender ID |
| 42 | 6 | reserved | 0 |

### `UsrspcAttr` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | size | bytes |
| 8 | 1 | extendable | 1 when writes past the end grow the space |
| 9 | 1 | initial_value | byte new space is filled with |
| 10 | 6 | reserved | 0 |

### `UsridxAttr` (32 bytes)

Entries live in a B+tree owned by the index, keyed by `IdxKey`. The
value starts with a tag byte: 0, and the entry's bytes after the key
follow (at most 1023 of them); 1, and an `IdxOverflowRef` names the
first of a chain of index entry overflow pages (type 0x0023, owned by
the index) that hold those bytes, 4048 per page. An entry is at most
2000 bytes, so `entry_len` is 1..2000.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | entry_kind | 0 fixed, 1 variable |
| 1 | 1 | reserved | 0 |
| 2 | 2 | key_len | 1..120 |
| 4 | 2 | entry_len | fixed length, or the maximum |
| 6 | 2 | reserved2 | 0 |
| 8 | 8 | root | tree root, 0 for empty |
| 16 | 8 | entry_count | |
| 24 | 8 | next_seq | |

### `IdxKey` (128 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 120 | key | the entry's first `key_len` bytes, zero padded |
| 120 | 8 | seq | insertion sequence, so equal keys sort by age |

### `IdxOverflowRef` (11 bytes)

The tree value of an entry whose data is on overflow pages.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | tag | 1 |
| 1 | 8 | page | the first overflow page |
| 9 | 2 | len | bytes of data in all |

### `IdxOverflowHeader` (16 bytes)

At offset 0 of an index entry overflow page, followed by `len` bytes
of the entry's data; `next` chains to the page holding the rest, 0 on
the last. Freed with the entry.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | next | next overflow page, 0 for none |
| 8 | 2 | len | data bytes on this page |
| 10 | 6 | reserved | 0 |

### `SavfAttr` (16 bytes)

A save file's byte space is an eight-byte big-endian length, then the
save stream (`docs/SAVE_FORMAT.md`); the stream counts as 528-byte
records against `MAXRCDS`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | max_records | `MAXRCDS`, 0 for `*NOMAX` |
| 4 | 4 | wait_file | `WAITFILE` seconds, signed; 0 `*IMMED`, -1 `*CLS` |
| 8 | 1 | share | 1 for `SHARE(*YES)` |
| 9 | 7 | reserved | 0 |

### `SysvalAttr` (16 bytes)

The byte space holds the `sysval` crate's TLV blob.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | blob_len | bytes |
| 8 | 8 | reserved | 0 |

## 11. Device parity protection

IBM i keeps device parity (RAID 5, RAID 6) in its SAS RAID controllers.
QS/400 runs on plain disks, so the store keeps it itself, as it keeps
mirroring. A unit is unprotected, half of a mirrored pair, or a member
of one parity set, never two of these.

**Sets.** A parity set is 3 to 18 units for RAID 5 (`*PARITY`, one
parity page per stripe) and 4 to 18 for RAID 6 (`*RAID6`, two), IBM's
limits for its current controllers. An ASP has at most 15 sets. The
members are numbered from 0 in `parity_member`; the set uses the size
of its smallest member, as IBM's controllers do: a larger member's rest
is held unused (the load source, which also carries the ESP and the
LIC partition, is usually the smallest). Membership, level and state live in the unit table
(§5, `parity_*`); the work under way lives in the superblock.

**Data stays where it is.** Each member stays its own span and keeps its
logical pages: nothing moves when protection starts or stops. Parity
takes the tail of each member instead, the share IBM's displays show as
the capacity a unit gives up. With `N` members, `k` parity pages per
stripe (1 or 2), `S` the smallest member's span pages and `SLOT` the
pages of one partial-parity slot (below):

- `A = S - 1 - 3 - 2 * SLOT - 1` pages are left for rows and parity;
- `B = A div N` blocks; `w = N - k` rows per block;
- the rows are member pages `3 .. 3 + B * w` (span-relative, which for
  a member is unit-relative): the data, the WAL on unit 0 included;
- the parity area is `3 + B * w .. 3 + B * N`, `k` pages per block;
- the two partial-parity slots follow, `SLOT` pages each, then the
  state page (`ParityState`);
- from there to the span's last page, nothing (a larger member's rest).

Pages from `3 + B * w` to the span's last page are held by the
allocator: never allocated, never in the free-space tree, never freed
by RCLSTG. Starting protection needs them to hold no data and no IFS
extent (§12). Pages 0, 1, 2
and the last page are the unit's own and are in no stripe.

**Stripes.** Block `b` holds `N` stripes `s = b * N + j`, `j` from 0 to
`N - 1`. Stripe `s` has its P page on member `j` at parity page
`3 + B * w + k * b`; for RAID 6 its Q page is on member `(j + 1) mod N`
at `3 + B * w + k * b + 1`. Every other member gives the stripe one data
page: member `u` takes part in the stripes of the block whose parity it
does not hold, in ascending `j`, and its `i`th such stripe is row
`3 + b * w + i`. So parity rotates over all members and each member
gives up `1/N` (RAID 5) or `2/N` (RAID 6) of its rows.

P is the XOR of the stripe's data pages. Q is `sum g^m * D_m` over the
data pages in GF(2^8) with the polynomial `0x11D` and `g = 2`, `m` the
member number of the page's unit. One unknown page of a stripe comes
from P (or from Q when P is unknown too); two come from P and Q.
Parity pages carry no trailer of their own: they are the XOR (and
product sum) of whole pages, trailers included.

### Parity unit states

| Value | State | Display | Meaning |
|---|---|---|---|
| 0 | Active | `DPY/Active` | in step with the set |
| 1 | Synchronizing | `DPY/Unprotected` | the set's parity is being computed: start, include, exclude |
| 2 | Rebuilding | `DPY/Rebuild` | a replaced or returned unit being rebuilt from the others |
| 3 | Failed | `DPY/Failed` | missing or failing; its pages are computed from the others |

The displays derive the other members' status from these: while a set
has a failed or rebuilding member, the others show `DPY/Unprotected`
for RAID 5 and for RAID 6 with two, `DPY/Degraded` for RAID 6 with one.

**Missing units.** At open a member that is not found is Failed when
its set was Active and no more than `k` members are Failed or
Rebuilding; the ASP opens exposed (an attention code) and reads of the
missing unit's pages are computed from the stripe. A member found again
after being Failed is stale and Rebuilding from block 0. More unknown
members than `k`, or any missing member of a set that is still
Synchronizing, stops the open (disk unit missing, B1001030). A member
whose I/O fails while running becomes Failed when the set can lose it;
otherwise the error reaches the store. Before anything is written
without it, the failure is recorded in the state page of every other
member (flushed): the unit table only learns of it at the next
checkpoint, and a crash before then must not let the stale unit be read
as in step. At open the newest valid state page turns an Active member
it names into a stale one (Rebuilding from block 0). A rebuild that
completes writes the state page again without the member.

**Background work.** One synchronization or rebuild runs at a time per
ASP, block by block in ascending order; the superblock records the set,
the kind, the member and `parity_work_done` as of each checkpoint.
Blocks below that mark are done and stay done: writes to them keep
parity. A synchronization first zeroes the partial-parity slots of every
member, a rebuild those of the member it rebuilds; then each block is
computed and written, and the members flushed, before the mark moves. A
crash resumes from the mark of the last checkpoint and redoes the blocks
after it, which is harmless. Sets waiting their turn keep their state;
the next one starts at block 0 when the current one ends.

The work runs with the system up, as IBM's controllers run it: the IPL
does not wait for it. The storage service does it between requests in
turns of a few milliseconds (short ones while requests wait), each turn
steps of 16 blocks while requests wait and otherwise of as many blocks
as make 16 MiB of rows over the members. A step reads each member's rows of its blocks
in one run and writes each member's parity pages in one run (they are
contiguous, see above), falling back to a stripe at a time when a
member is not in step or a read fails. The mark is recorded by a
checkpoint each time it has moved a thousandth of the set's blocks (at
least 32) past the one last recorded, and when the work ends; power
down's closing checkpoint records it too. In memory the mark moves only
after the step's pages are flushed, and a checkpoint flushes before it
writes the superblock, so the recorded mark never runs ahead of the
units.

While the work runs, the rules are:

- A step holds the set for its blocks: it first makes the writes held
  so far (each to its stripe by the rules below, as of the old mark),
  then computes its blocks, so no write lands in a block while it is
  computed. A write held before the step is computed with its block; a
  write after it finds the block below the mark.
- Synchronization: a write to a stripe below the mark is made in an
  epoch with its partial parity, like any write to a protected set; a
  write above the mark goes to its data page only. A crash between two
  recorded marks leaves the blocks between them with either kind of
  write. The open repairs the stripes the newest slots name, and the
  synchronization then computes those blocks again from their data.
- Rebuild: a page of the unit being rebuilt is read from the unit below
  the mark and computed from the rest of its stripe above it. Writes go
  to every member in step or being rebuilt, the new unit included, with
  partial parity on the parity members, so a block once rebuilt stays
  current. After a crash the unit is in step only below the recorded
  mark; the blocks above are rebuilt again from the others.
- A member missing at open while its set synchronizes stops the open
  (the set has no parity yet); during a rebuild the rules of "Missing
  units" above apply, the member being rebuilt counting as unknown.

Pages that start being held (start, include, exclude) are written only
after the checkpoint that records the new set: until then the last
durable superblock may still reach them. Pages that stop being held
become free at that checkpoint, as freed pages do (§7).

**Writes and the write hole.** A data page is never written in place
on its own. Writes to a set are held until the next flush, then made
in epochs of at most 126 stripes:

1. For every stripe written, the old value of each data page is read
   (computed from the stripe when its unit is not in step), and the
   partial parity is computed: the P (and for RAID 6 the Q) of the
   pages the epoch does not write. The new P and Q follow from the
   old values and the new pages.
2. The partial parity goes to the next slot of each parity member of
   those stripes, which are flushed.
3. The data pages and the new parity pages are written in place and
   every member is flushed.

A crash in step 3 can leave a stripe with new data and old parity, or
the reverse: the RAID write hole. At open, before anything is read,
the newest valid slot of every member that can be trusted (not Failed;
a Rebuilding one for blocks below its mark) is read and each stripe it
names is repaired: the pages the epoch did not write are known from the
partial parity even when one (RAID 6: two) of their units is missing,
the pages it wrote are what their units hold now (for a missing unit,
what the stripe's parity says, else zeros), and P and Q are written
afresh. Nothing the store committed lies on the pages an epoch wrote
without the WAL covering them (§6: copy-on-write pages are flushed
before their record commits, in-place pages are imaged in the WAL, an
unfinished WAL page ends the log), so the store sees at most the torn
writes it already tolerates. Stripes of an earlier epoch were flushed
before the next epoch's slots were written; repairing one again
changes nothing. Pages the store writes to a member outside the rows
(unit header, superblock copies) go straight to the unit.

Writes to a stripe of a set still Synchronizing, above the mark, go to
the data page only; the synchronization computes their parity when it
gets there.

**Reads.** A data page is read from its unit when the unit is in step
for that stripe. When it is not, or the read fails, or the page is not
zero and its trailer checksum fails (never asked of an IFS volume page,
which has no trailer, §12), it is computed from the rest of the stripe; a page computed this way that verifies is written back to
its unit when the unit answers.

### `PplHeader` (64 bytes)

At offset 0 of the first page of a partial-parity slot (a raw page, no
trailer). The entries follow from offset 64; the partial pages of entry
`i` are pages `1 + 2 * i` (P) and `2 + 2 * i` (Q, RAID 6) of the slot. A
slot is `SLOT = 1 + 2 * 126 = 253` pages. A member writes its two slots
in turn; the valid one with the higher `seq` is the newest.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400PPL` |
| 8 | 8 | seq | epoch number, never reused while the set exists |
| 16 | 16 | asp_uuid | |
| 32 | 1 | set | parity set |
| 33 | 1 | level | 5 or 6 |
| 34 | 1 | members | `N` |
| 35 | 1 | member | this unit's member number |
| 36 | 4 | count | entries, at most 126 |
| 40 | 8 | blocks | `B` |
| 48 | 4 | crc | CRC32C of this header with `crc` zero, the entries and the partial pages |
| 52 | 12 | reserved | 0 |

### `PplEntry` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | stripe | `b * N + j` |
| 8 | 4 | written | bit `m` set: member `m`'s data page of the stripe is written in this epoch |
| 12 | 4 | reserved | 0 |

### `ParityState` (64 bytes)

At offset 0 of the state page, `3 + B * N + 2 * SLOT` on every member (a
raw page, no trailer). Every member carries the same contents; the
valid one with the highest `seq` is the newest. A synchronization zeroes
it.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400PST` |
| 8 | 8 | seq | write number |
| 16 | 16 | asp_uuid | |
| 32 | 1 | set | parity set |
| 33 | 1 | level | 5 or 6 |
| 34 | 1 | members | `N` |
| 35 | 1 | reserved1 | 0 |
| 36 | 4 | failed | bit `m` set: member `m` has failed |
| 40 | 8 | blocks | `B` |
| 48 | 4 | crc | CRC32C of these 64 bytes with `crc` zero |
| 52 | 12 | reserved | 0 |

## 12. The IFS in the ASP

The IFS root's xfs (docs/IFS.md "Disk format") lives on pages of the
ASP set aside for it: the IFS volume. The storage service serves the
volume to the kernel as a block device; the store's mirroring and
device parity protect it like any other page, and it grows into the
ASP's free space. Stores formatted before this section have
`ifs_map` 0 and keep their IFS in its own partition (docs/IFS.md).

**Extents.** The volume is a list of extents, each a run of logical
pages within one span. Volume page `v` is page `v - s` of the extent
whose first volume page `s` is the largest not above `v`, extents taken
in map order. Extents never overlap one another, the reserved pages of
a span (0, 1, 2, the WAL, the last page) or the pages device parity
holds. Volume pages are raw: all 4096 bytes are the filesystem's, with
no trailer and no checksum the store checks (xfs checksums its own
metadata). They never pass through the page cache or the WAL.

**Holding.** The allocator holds every extent (§11 describes holds):
never handed out, never free in the tree, never freed by RCLSTG, never
moved. Holds are not persisted; at open the ASP holds each extent the
map lists before anything allocates. Held pages count as used in the
ASP's figures.

**Placement.** An extent added while its span is a device parity member
comes with one extent on every member of the set at the same rows,
covering whole blocks: rows `3 + b0 * w .. 3 + b1 * w` of each member
for blocks `b0 .. b1`. Every stripe of those blocks then holds volume
pages only. Extents placed before a set was started keep their pages:
the set treats each volume page as raw wherever it lies. Elsewhere an
extent is any free run of a span. Placement takes the lowest addresses
that fit, in unit order, and runs of at least 256 pages (whole blocks
in a set), so the map stays short.

**Creation.** When an ASP that has no IFS map opens at IPL and no IFS
partition exists, the IFS is created: a tenth of the ASP's usable pages
(device parity's held pages excluded), at least 262144 pages (1 GiB),
never past the ASP threshold (below), and not at all when fewer than
16384 pages (64 MiB) would fit. The extents are held, the map is
written and a checkpoint publishes it; then the filesystem is formatted
on the volume and a second checkpoint sets the superblock's
`ifs_formatted` flag. A crash before the first checkpoint leaves no IFS
and the next IPL starts again; a crash before the second formats the
volume again. A volume whose flag is set is never formatted again.

**Growth.** The volume grows by extents appended to the map; extents
are never removed and the volume never shrinks. A growth is a step of
2% of the ASP's usable pages, rounded up to whole blocks inside a
parity set, or what is left below the ASP threshold when that is less
(at least 16384 pages; otherwise there is no growth). The ASP threshold
is the percentage of the ASP's usable pages that may be in use, 90 by
default (DST's Work with ASP threshold); held IFS pages count as used.
The new extents are held, a new map is written and a checkpoint
publishes it before the volume's new size is given to the kernel. A
crash before the checkpoint loses the growth and nothing else; the
filesystem only learns of the new pages after it.

**Unit changes.** A unit whose span holds an IFS extent cannot leave the
configuration, and cannot be paired when pairing would move its pages
or shorten its span below an extent's end. Starting device parity over
units is refused when the pages the set would hold (§11) overlap an IFS
extent. Mirror resumes copy volume pages as they copy every held page;
parity synchronization and rebuild cover them as they cover every row.

**The map.** A chain of map pages (type 0x0012, ordinary pages with a
trailer, owner zero), each a header and up to 252 extents. The map is
never changed in place: a change writes a new chain on pages the
allocator hands out, frees the old chain's pages and checkpoints; the
superblock's `ifs_map` and `ifs_pages` name the new chain. A crash
before the checkpoint leaves the old chain in force, its pages still
allocated in the durable free tree.

### `IfsMapHeader` (32 bytes)

At offset 0 of the body of each map page.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400IFS` |
| 8 | 8 | next | next map page, 0 for the last |
| 16 | 2 | count | extents in this page, at most 252 |
| 18 | 6 | reserved | 0 |
| 24 | 8 | first_volume_page | volume page of this page's first extent |

### `IfsExtent` (16 bytes)

From body offset 32, `count` entries, in volume order.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | start | first logical page |
| 8 | 8 | pages | length; the run lies within one span |

## Frozen at V1R1M0

Frozen: the page and its trailer, the unit label, the superblock, the WAL pages and records, B+tree nodes and their key and value codecs, the allocation bitmaps and free log, the parity table and partial-parity log, the IFS map, and the object header with the type codes and the payloads the store itself defines (authority, data areas, data queues, user indexes, system values). The versions are the superblock's and unit's `FORMAT_VERSION` and the object header's `HEADER_VERSION`.

Changes require a format version bump and a migration: a reader of the old version kept beside the new one, or a conversion run at IPL or on restore, and a row in the release notes. `tools/fmtcheck` holds the frozen definitions (`tools/fmtcheck/formats.toml`) and their hashes (`formats.lock`); `cargo test -p fmtcheck` and `build test` fail when a definition changes and its version does not.
