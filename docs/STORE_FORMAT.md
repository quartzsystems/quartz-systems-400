# QS/400 object store: on-disk format

Format version 1. This document is written before the code that
implements each structure, and the code's constants are checked against
the tables here by `store/tests/spec.rs`. Every table under a heading of
the form `### \`Name\` (N bytes)` is parsed by that test: the heading name
must match a `Layout` in the code, and the `Offset`, `Size` and `Field`
columns must match its fields exactly. Prose and `Notes` are free.

## 0. Conventions

- A page is 4096 bytes. A unit is addressed in pages; an ASP is the
  concatenation of its units into one logical page space. `PageNo` is a
  logical page number in that space (`u64`).
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
(at most 256 bytes); values are variable (at most 1024 bytes). Nodes are
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
trailer CRC covers them. Neither page is ever allocated.

### `UnitHeader` (80 bytes)

At offset 0 of the body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400ASP` |
| 8 | 4 | format_version | 1 |
| 12 | 4 | flags | reserved, 0 |
| 16 | 16 | unit_uuid | |
| 32 | 16 | asp_uuid | |
| 48 | 2 | asp_number | 1 for SYSBAS |
| 50 | 2 | unit_number | position in the ASP, from 0 |
| 52 | 4 | reserved | 0 |
| 56 | 8 | pages | unit size in pages |
| 64 | 16 | mirror_of | unit UUID of the mirror source, zero for none; mirroring is not implemented |

## 5. Superblock

Two copies per unit, at pages 1 and 2. A checkpoint with generation `g`
writes copy `g mod 2` on every unit, so a torn write can only damage the
copy that was already stale. On open the newest copy whose page verifies,
across all units, wins. Opening a store whose creating release differs
in V or R from the running build is refused (CPF3294); M may differ.

Logical page numbers: unit 0 is pages `0..pages_0`, unit 1 continues at
`pages_0`, and so on (`start` in the unit table). Pages 0, 1, 2 and the
last page of every unit are reserved; the WAL ring is `wal_pages` pages
of unit 0 from page `wal_start` (3).

### `Superblock` (256 bytes)

At offset 0 of the body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400SUP` |
| 8 | 4 | format_version | 1 |
| 12 | 4 | flags | bit 0 `needs_recovery`, bit 1 `rclstg_required` |
| 16 | 8 | generation | checkpoint number; newest valid copy wins |
| 24 | 16 | asp_uuid | |
| 40 | 2 | asp_number | |
| 42 | 2 | unit_count | 1..119 |
| 44 | 1 | release_v | creating release |
| 45 | 1 | release_r | |
| 46 | 1 | release_m | |
| 47 | 1 | reserved | 0 |
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

The unit table follows at body offset 256: `unit_count` entries.

### `UnitEntry` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | unit_uuid | must match the unit header |
| 16 | 8 | pages | unit size in pages |
| 24 | 8 | start | logical page number of the unit's page 0 |

## 6. WAL

`TODO(phase2)`: written before transactions.

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

## 8. Object header

`TODO(phase2)`: written before the object layer.
