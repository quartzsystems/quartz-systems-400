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
  is one unprotected unit or one mirrored pair of units holding the same
  pages. `PageNo` is a logical page number in that space (`u64`).
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
| 0x0010 | B+tree node (leaf or internal, see the node header) |
| 0x0020 | object header |
| 0x0021 | object data |
| 0x0022 | extent overflow |

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
(at most 1024 bytes); values are variable (at most 1024 bytes). A tree may fix its key size at run time (file access paths do). Nodes are
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
| 50 | 2 | unit_number | position in the unit table, from 0 |
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
when that copy verifies, rewritten on the first. A half that fails I/O
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
is refused (CPF3294); M may differ.

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
| 12 | 4 | flags | bit 0 `needs_recovery`, bit 1 `rclstg_required` |
| 16 | 8 | generation | checkpoint number; newest valid copy wins |
| 24 | 16 | asp_uuid | |
| 40 | 2 | asp_number | |
| 42 | 2 | unit_count | 1..47 |
| 44 | 1 | release_v | creating release |
| 45 | 1 | release_r | |
| 46 | 1 | release_m | |
| 47 | 1 | protection | 0 no unit mirrored, 1 at least one pair |
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
| 168 | 88 | reserved2 | 0 |

The unit table follows at body offset 256: `unit_count` entries, one
per unit in unit-number order. Units are added, removed and paired only
through the service tools; the table never changes on its own.

### `UnitEntry` (80 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | unit_uuid | must match the unit header |
| 16 | 8 | pages | unit size in pages (the device size) |
| 24 | 8 | start | logical page number of the span's page 0 |
| 32 | 8 | usable_pages | pages of the span: `pages`, or the smaller of a pair |
| 40 | 2 | mirror_partner | unit number of the other half, 0xFFFF when unprotected |
| 42 | 1 | state | see §4, unit states |
| 43 | 5 | reserved | 0 |
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
| 8 | 4 | header_version | 1 |
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
| 840 | 184 | reserved6 | 0 |

Public authority bits: 0x0001 `*OBJOPR`, 0x0002 `*OBJMGT`, 0x0004
`*OBJEXIST`, 0x0008 `*OBJALTER`, 0x0010 `*OBJREF`, 0x0020 `*READ`,
0x0040 `*ADD`, 0x0080 `*UPD`, 0x0100 `*DLT`, 0x0200 `*EXECUTE`, 0x0400 `*AUTLMGT`; 0 is
`*EXCLUDE`; 0x8000_0000 means the public authority comes from the
authorization list. Phase 6 enforces them; `docs/SECURITY.md` has the
search order. The private authority overflow tree's pages are B+tree
nodes owned by the object; RCLSTG walks it and delete frees it.

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

**Owned objects.** An object whose `library` field names an object that
is not a library belongs to that object and has no library directory
entry; it is reached through its owner and listed only in the object
directory. RCLSTG keeps it while the owner exists and removes it, pages
and trees included, when the owner is gone.

## 10. Payload types

An object's payload lives in its extents. For the types below the
extents form one byte space: page `i` of the concatenated extents holds
bytes `i * 4064 ..` of the space (the body of a data page, type
0x0021, owned by the object). Changes to the payload are in-place
changes logged as WAL images and patches (§6); they never move a page,
so the extent list stays put. `TODO(perf)`: user space writes that
cover whole pages could skip the patch and image the page directly.

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

Entries live in a B+tree owned by the index, keyed by `IdxKey`, whose
value is the entry's bytes after the key (at most 1024, so
`entry_len` is at most `key_len + 1024`; longer entries are a later
phase).

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

### `SysvalAttr` (16 bytes)

The byte space holds the `sysval` crate's TLV blob.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | blob_len | bytes |
| 8 | 8 | reserved | 0 |
