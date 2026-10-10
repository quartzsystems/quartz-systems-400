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

`TODO(phase2)`: written before the allocator.

## 5. Superblock

`TODO(phase2)`: written before the allocator.

## 6. WAL

`TODO(phase2)`: written before transactions.

## 7. Object header

`TODO(phase2)`: written before the object layer.
