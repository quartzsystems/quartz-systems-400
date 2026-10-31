# QS/400 save format

The save stream every `SAV*` command writes and every `RST*` command
reads, on any medium: a save file, a tape file, a file on an optical
volume, an install medium, a PTF. Format version 1. This is Quartz's
own format; IBM save files are not restorable and IBM systems cannot
read ours.

This document is written before the code. `save/savfmt/tests/spec.rs`
compares the code's `Layout`s with the tables here the way
`storage/store/tests/spec.rs` does for `STORE_FORMAT.md`: every table
under a heading `### \`Name\` (N bytes)` must match a layout in the
code, field for field.

## 0. Conventions

- Integers are big-endian. A stream is read on other systems and other
  releases, and IBM i's own interfaces are big-endian; nothing here is
  mapped into memory.
- Name and text fields are CP037, blank padded, like object headers.
  The command parameters and IFS paths are UTF-8.
- CRC32C (Castagnoli) everywhere, as in the store.
- Offsets in the stream count bytes from the first byte of the media
  header, across volumes: a stream continued on a second tape volume
  keeps counting.

## 1. The stream

```
MediaHeader
Record (Parameters)
Record (Object) [Record (Authorities)] Record (Data)* Record (ObjectEnd) [Record (Signature)]
...                                       one group per object, in save order
Record (Index)*                           the object index, 16,384 entries per record
Record (Trailer)
```

A reader that only moves forward (a tape read without positioning, a
pipe) sees every object in order and builds the index as it goes. A
reader that can seek reads the trailer first: it is the last record
and has a fixed size, so it sits `RecordHeader + Trailer` bytes from
the end. The trailer points at the index; the index gives every
object's offset. `DSPSAVF`, `DSPTAP DATA(*SAVRST)` and a restore of
selected objects use that path and read only what they show or restore.

### `MediaHeader` (128 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400SAV`, ASCII |
| 8 | 2 | format_version | 1 |
| 10 | 2 | flags | reserved, 0 |
| 12 | 1 | release_v | release of the saving system |
| 13 | 1 | release_r | |
| 14 | 1 | release_m | |
| 15 | 1 | target_v | `TGTRLS`: the oldest release that may restore it |
| 16 | 1 | target_r | |
| 17 | 1 | target_m | |
| 18 | 1 | compression | `DTACPR`: 0 `*NO`, 1 `*LOW`, 2 `*MEDIUM`, 3 `*HIGH` |
| 19 | 1 | kind | see below |
| 20 | 4 | reserved | 0 |
| 24 | 8 | created | Unix seconds when the save started |
| 32 | 16 | stream_id | unique per save; repeated in the trailer |
| 48 | 8 | system | saving system's name |
| 56 | 10 | serial | saving system's serial number |
| 66 | 10 | command | `SAVLIB`, `SAVOBJ`, `SAV`, ... |
| 76 | 10 | user | the user who saved |
| 86 | 38 | reserved2 | 0 |
| 124 | 4 | crc | CRC32C of bytes 0..124 |

Kinds: 0 library save (`SAVLIB`), 1 object save (`SAVOBJ`), 2
changed objects (`SAVCHGOBJ`), 3 integrated file system (`SAV`), 4
system (`SAVSYS`), 5 security data (`SAVSECDTA`), 6 configuration
(`SAVCFG`), 7 licensed program (`SAVLICPGM`, product loads), 8 PTF,
9 save file data (`SAVSAVFDTA`: the saved stream of a save file).

`TGTRLS`: within V1R0 every stream is compatible. A restore refuses a
stream whose target release is newer than the running release, and a
stream whose format version it does not know (CPF3743-style). `*PRV`
names V1R0M0 until a V1R1M0 exists.

### `RecordHeader` (24 bytes)

Every record after the media header.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | mark | `QR`, ASCII: a reader resynchronizes on it after damage |
| 2 | 1 | kind | see the table below |
| 3 | 1 | codec | 0 stored, 1 LZ4 block, 2 zstd frame; `Data` only |
| 4 | 4 | seq | record number from 0; gaps and repeats are damage |
| 8 | 4 | stored_len | payload bytes that follow |
| 12 | 4 | raw_len | payload bytes after decompression, at most 1,048,576 |
| 16 | 4 | crc | CRC32C of the stored payload |
| 20 | 4 | header_crc | CRC32C of bytes 0..20 |

| Kind | Record | Payload |
|---|---|---|
| 0x01 | Parameters | the command as typed, UTF-8, `KEYWORD(value)` form, defaults filled in |
| 0x10 | Object | `ObjectDescriptor`, then `tail_len` bytes |
| 0x11 | Authorities | `AuthorityHeader`, then `count` × `AuthorityEntry` |
| 0x12 | Data | `ChunkHeader`, then the part's bytes |
| 0x13 | ObjectEnd | `ObjectEnd` |
| 0x14 | Signature | `SignatureRecord` |
| 0x20 | Index | `count` × `IndexEntry` (`count` = `raw_len / 64`) |
| 0x21 | Trailer | `Trailer` |

Compression applies to `Data` payloads only, chunk by chunk, and only
when it makes the chunk smaller: a chunk that does not shrink is stored
(`codec` 0) whatever `DTACPR` says. `*LOW` is LZ4 (`lz4_flex`);
`*MEDIUM` and `*HIGH` are zstd (`ruzstd`), whose encoder implements one
level, its fastest, so the two are the same today and a later encoder
level changes only the bytes, never the format. Both crates are pure
Rust and pinned. The CRC covers the stored bytes,
so damage is found before anything is decompressed.

## 2. Objects

One group of records per object. The library object of a library save
comes first, then its objects in save order (§4); an object that owns
others (a file and its members) is followed directly by them, each
naming its owner by index.

### `ObjectDescriptor` (512 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | index | the object's position in the stream, from 0 |
| 4 | 4 | owner_index | the owning object's index; 0xFFFFFFFF for none |
| 8 | 2 | type_code | the store's type registry; 0 for an IFS object |
| 10 | 2 | flags | see below |
| 12 | 1 | domain | 0 `*SYSTEM`, 1 `*USER` |
| 13 | 1 | state | 0 system, 1 user |
| 14 | 2 | reserved | 0 |
| 16 | 16 | original_id | the object's `ObjectId` on the saving system |
| 32 | 16 | library_id | the `ObjectId` of its library or owner there |
| 48 | 10 | library | library name, blank for an IFS object |
| 58 | 10 | name | object name, blank for an IFS object |
| 68 | 10 | attribute | `PF`, `CLLE`, ... |
| 78 | 50 | text | |
| 128 | 10 | owner | owning profile |
| 138 | 10 | primary_group | blank for none |
| 148 | 10 | creator | |
| 158 | 8 | system | system the object was created on |
| 166 | 1 | release_v | creating release |
| 167 | 1 | release_r | |
| 168 | 1 | release_m | |
| 169 | 7 | reserved2 | 0 |
| 176 | 8 | created | Unix seconds |
| 184 | 8 | changed | |
| 192 | 8 | saved | this save, Unix seconds |
| 200 | 8 | restored | last restore on the saving system, 0 for never |
| 208 | 8 | last_used | |
| 216 | 4 | days_used | |
| 220 | 4 | public_authority | the store's authority bits |
| 224 | 10 | autl | authorization list, blank for none |
| 234 | 10 | audit | `OBJAUD`, blank for `*NONE` |
| 244 | 4 | owner_authority | bits, meaningful when `auth_flags` bit 0 is set |
| 248 | 4 | group_authority | bits |
| 252 | 4 | auth_flags | the header's `auth_flags` |
| 256 | 8 | size_bytes | raw bytes of every part |
| 264 | 2 | part_count | parts that follow in `Data` records |
| 266 | 2 | tail_len | bytes after the descriptor: the IFS path |
| 268 | 4 | owned_count | owned objects that follow this one |
| 272 | 240 | reserved3 | 0 |

Flags: bit 0 the program adopts its owner's authority, bit 1 a
signature record follows, bit 2 an IFS object (the tail is its
absolute path, UTF-8), bit 3 an authorities record follows, bit 4 the
data was not saved (`SAVFDTA(*NO)`, `QDTA(*NONE)`: the object is
restored empty), bit 5 the object's storage was freed by the save
(`STG(*FREE)`).

### `AuthorityHeader` (8 bytes)

The object's private authorities, kept apart from the descriptor
because a restore applies them separately: `RSTOBJ` and `RSTLIB`
restore them only with `PVTAUT(*YES)`; otherwise they come back only
through `RSTUSRPRF` and `RSTAUT` as on IBM i.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | index | object |
| 4 | 4 | count | entries that follow |

### `AuthorityEntry` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | profile | |
| 10 | 4 | bits | the store's authority bits |
| 14 | 2 | reserved | 0 |

### `ChunkHeader` (24 bytes)

An object's payload is a set of parts (§3). Each part is written as
chunks of at most 1 MiB of raw bytes, in offset order; a reader
assembles them and checks that each part's chunks cover it exactly.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | index | object |
| 4 | 1 | part | part kind, §3 |
| 5 | 3 | reserved | 0 |
| 8 | 8 | offset | byte offset of this chunk within the part |
| 16 | 8 | part_len | the whole part's length, repeated in every chunk |

The `raw_len` of a data record counts the chunk header too; the
header is never compressed (it is written in front of the compressed
bytes and the codec covers what follows it).

### `ObjectEnd` (48 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | index | object |
| 4 | 2 | parts | parts written |
| 6 | 2 | reserved | 0 |
| 8 | 8 | bytes | raw bytes of every part |
| 16 | 32 | digest | SHA-256, §5 |

### `SignatureRecord` (112 bytes)

Present for objects that carry a signature (every object Quartz ships;
`QVFYOBJRST` decides what a restore does with it, docs/SECURITY.md).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | index | object |
| 4 | 2 | algorithm | 1 ed25519 |
| 6 | 2 | reserved | 0 |
| 8 | 8 | key_id | first 8 bytes of the SHA-256 of the public key |
| 16 | 64 | signature | over `digest` |
| 80 | 32 | digest | the object's digest, as in `ObjectEnd` |

## 3. Parts

What an object's payload is depends on its type; the engine
(`save/saverst`) reads and writes it. The parts are:

| Kind | Part | Contents |
|---|---|---|
| 1 | attr | the 1024-byte attribute area, with page numbers zeroed |
| 2 | space | the byte space, `size` bytes |
| 3..6 | tree0..tree3 | the B+tree in header tree slot 0..3 as `TreeImage` |
| 7 | refs | references to other objects by name, `RefEntry` list |
| 8 | spool | the spooled files of an output queue (`SPLFDTA(*ALL)`) |
| 9 | ifsmeta | an IFS object's metadata record (`ifs::meta` codec) |
| 10 | ifsdata | an IFS stream file's bytes, or a symbolic link's target |
| 16..31 | typed | a type's own export (a user index's entries, a profile's indexes) |

A part that is empty is not written. Page numbers never leave the
system: an attribute field holding a page (a library's directory root,
a user index's root) is zeroed on save and rebuilt on restore, and a
tree whose values hold page numbers (a user index's overflow entries)
is saved as a typed export instead of a tree image.

### `TreeImage` (variable)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | key_len | bytes per key |
| 2 | 2 | reserved | 0 |
| 4 | 4 | count | entries |
| 8 | variable | entries | `count` × (key, `u16` value length, value), in key order |

### `RefEntry` (64 bytes)

Object IDs inside a payload are the saving system's, and a restore
gives every object a new one. The saver finds each ID in the parts that
carry them (the attribute area of every object; the byte space of a
`*FILE` and a `*JRN`, which hold definitions, never records) and lists
it here with where it sits and the referenced object's name. A restore
patches each listed offset: with the new ID of an object restored from
the same stream, or with the ID of the object of that name already on
the system (a logical file over a physical file in another library, a
file journaled to a journal elsewhere), as `flags` says below. Nothing is searched on
restore, so record data can never be mistaken for an ID. An ID found
nowhere is zeroed, and the type's rule decides what that means (§6).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | original_id | the referenced object's ID on the saving system |
| 16 | 2 | type_code | its type |
| 18 | 2 | region | 1 the attribute area, 2 the byte space |
| 20 | 10 | library | its library name (for a member, its file's) |
| 30 | 10 | name | its name (for a member, its file's) |
| 40 | 10 | member | member name for a `*MBR`, blank otherwise |
| 50 | 8 | offset | byte offset of the ID within the region |
| 58 | 2 | flags | bit 0 the object is in the same stream, bit 1 it comes earlier in it |
| 60 | 4 | reserved | 0 |

How a restore resolves a reference: to the object restored from the
stream when there is one; otherwise, for an object earlier in the
stream that this restore did not bring back (`OBJ` left it out) or one
outside the stream, by name: in the restore library first when
`RSTLIB` renames, then in the saved library. A reference to an object
later in the stream that is never restored is zeroed, never resolved by
name: a physical file's dependents and a journal's objects name things
that depend on them, and an unrelated object of the same name must not
take their place.

## 4. Index and trailer

### `IndexEntry` (64 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | index | object |
| 4 | 4 | owner_index | 0xFFFFFFFF for none |
| 8 | 8 | offset | stream offset of the object's `Object` record header |
| 16 | 2 | type_code | |
| 18 | 2 | flags | the descriptor's flags |
| 20 | 10 | library | |
| 30 | 10 | name | |
| 40 | 10 | attribute | |
| 50 | 6 | reserved | 0 |
| 56 | 8 | size_bytes | |

### `Trailer` (64 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | objects | objects in the stream |
| 4 | 4 | not_saved | objects the save named but could not save |
| 8 | 8 | index_offset | stream offset of the first `Index` record header, 0 for none |
| 16 | 4 | records | records before the trailer |
| 20 | 4 | index_records | `Index` records |
| 24 | 8 | bytes | stream bytes before the trailer's record header |
| 32 | 16 | stream_id | as in the media header |
| 48 | 16 | reserved | 0 |

A stream without its trailer is incomplete: the save ended before it
finished, or a volume is missing. A restore from it restores the
objects that are complete, reports the rest as not restored, and ends
with an escape message; `DSPSAVF` shows the objects it can read and
says the save is incomplete.

## 5. Integrity

Damage is found at three levels: each record's header CRC and payload
CRC; each object's digest; the stream's sequence numbers and trailer.
The digest covers the object's content and nothing an operator
changes: the SHA-256 of the ASCII bytes `QS400OBJ`, `type_code` (2
bytes), `attribute` (10), `domain`, `state` and the adopt flag (1 byte
each), then for each part in kind order the part kind (1 byte), its
length (8 bytes) and its bytes. Names, owners, authorities, dates and
usage are left out, so a signature made when the object was built
still verifies after a rename, a change of owner, use, and any number
of saves and restores. The descriptor itself is protected by its
record's CRC.

A reader:

- stops at the first record whose header CRC or mark is wrong, or
  whose `seq` is not the next, and reports the objects before it;
- treats a payload CRC failure as damage to that object only, skips
  to its `ObjectEnd`, and continues;
- fails an object whose parts do not cover their lengths or whose
  digest differs.

## 6. Save order and references (the engine)

A library save writes the library, then its objects in this order, so
a restore in stream order always finds what an object depends on:

1. `*JRN` journals, then `*JRNRCV` receivers oldest first;
2. physical files (`PF`, source files), each followed by its members;
3. logical files, those over files of the same library after them,
   each followed by its members;
4. every other type, by type code then name;
5. DDM files last.

Type-specific contents (`save/saverst/src/image.rs`):

| Type | Parts |
|---|---|
| `*LIB` | attr (directory root and object count zeroed) |
| `*FILE` | attr, space, refs; each member follows as an owned object |
| `*MBR` | attr, space (the record slots), tree0 (access path), tree1 (free slots), refs |
| `*USRIDX` | attr (root and entry count zeroed), typed 16: every entry in key order, each a `u16` length and the entry |
| `*USRPRF` | attr, space, typed 16: its authority table, `AuthTableEntry` rows (`SAVSECDTA`) |
| `*AUTL` | attr, space, the authorities record: its entries |
| `*OUTQ` | attr, space, refs; spool with `SPLFDTA(*ALL)`: a `u32` count, then per spooled file a `u32` length and its attribute record (`work/wm` `SpooledFile`), a `u64` length and its data |
| every other type | attr, space, refs, tree0..tree3 when the header has them |

A profile's indexes (objects it owns, is authorized to, is the group
of) and the index of objects an authorization list secures are this
system's and are never saved; they are rebuilt as objects are restored.
A profile's private authorities travel in its authority table instead,
by name, as IBM i's do: `RSTUSRPRF` keeps the table pending and `RSTAUT`
grants what it can, keeping the rest for a later `RSTAUT`.

### `AuthTableEntry` (36 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | library | the object's library (a member's file's) |
| 10 | 10 | name | the object's name (a member's file's) |
| 20 | 10 | member | member name, blank for an object |
| 30 | 2 | type_code | |
| 32 | 4 | bits | the store's authority bits |

When a reference cannot be resolved: a file's journal is dropped and
the file is restored not journaled; a logical member whose based-on
member is missing is not restored; a physical file's dependents and a
journal's object list lose entries whose objects are gone.

On restore, objects are created fresh (new IDs, new pages). The engine
keeps `original_id → new id` for everything it restored from the
stream, rewrites each object's references through that map or by name,
and rebuilds what it zeroed: the library directory, access paths after
their members (rebuilt from the records when a tree image is absent),
a user index's tree from its entries.

## 7. Outfiles

`OUTPUT(*OUTFILE)` of the save and restore commands writes IBM's model
formats, because programs read them by field name and position: the
formats are an interface, not part of the save stream. Each table's
Notes give the DDS type, length and decimals; varying fields hold a
two-byte length then the data.

### `QSRSAV` (3732 bytes)

Record format of `QASAVOBJ`, the save outfile, as IBM i V7R1M0's `DSPFFD` shows it.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | SROCMD | CHAR 10 |
| 10 | 10 | SROINF | CHAR 10 |
| 20 | 8 | SROSYS | CHAR 8 |
| 28 | 6 | SROSRL | CHAR 6 |
| 34 | 10 | SROLIB | CHAR 10 |
| 44 | 2 | SROASP | ZONED 2 0 |
| 46 | 6 | SROSAV | ZONED 6 0 |
| 52 | 6 | SROERR | ZONED 6 0 |
| 58 | 4 | SROSEQ | ZONED 4 0 |
| 62 | 17 | SROLBL | CHAR 17 |
| 79 | 60 | SROVOL | CHAR 60 |
| 139 | 13 | SROSVT | CHAR 13 |
| 152 | 10 | SRONAM | CHAR 10 |
| 162 | 10 | SROMNM | CHAR 10 |
| 172 | 8 | SROTYP | CHAR 8 |
| 180 | 10 | SROATT | CHAR 10 |
| 190 | 15 | SROSIZ | ZONED 15 0 |
| 205 | 10 | SROOWN | CHAR 10 |
| 215 | 1 | SROSTA | CHAR 1 |
| 216 | 7 | SROMSG | CHAR 7 |
| 223 | 13 | SROSWA | CHAR 13 |
| 236 | 50 | SROTXT | CHAR 50 |
| 286 | 40 | SRODEV | CHAR 40 |
| 326 | 10 | SROSVF | CHAR 10 |
| 336 | 10 | SROSFL | CHAR 10 |
| 346 | 6 | SROTRL | CHAR 6 |
| 352 | 1 | SROSTF | CHAR 1 |
| 353 | 1 | SROACP | CHAR 1 |
| 354 | 1 | SROSFD | CHAR 1 |
| 355 | 1 | SROCMP | CHAR 1 |
| 356 | 1 | SROCOM | CHAR 1 |
| 357 | 7 | SRORFD | CHAR 7 |
| 364 | 6 | SRORFT | CHAR 6 |
| 370 | 7 | SROEXP | CHAR 7 |
| 377 | 392 | SROXVM | VARCHAR 390 |
| 769 | 10 | SROPGP | CHAR 10 |
| 779 | 10 | SROSQ2 | ZONED 10 0 |
| 789 | 1 | SROMIT | CHAR 1 |
| 790 | 1 | SROFMT | CHAR 1 |
| 791 | 3 | SROMFN | ZONED 3 0 |
| 794 | 3 | SROTMF | ZONED 3 0 |
| 797 | 10 | SROMDN | CHAR 10 |
| 807 | 10 | SROMDL | CHAR 10 |
| 817 | 3 | SROVLC | ZONED 3 0 |
| 820 | 3 | SROVLL | ZONED 3 0 |
| 823 | 2402 | SROVLD | VARCHAR 2400 |
| 3225 | 258 | SROOPT | VARCHAR 256 |
| 3483 | 10 | SROAS1 | CHAR 10 |
| 3493 | 5 | SROAS2 | ZONED 5 0 |
| 3498 | 11 | SROTSZ | PACKED 21 0 |
| 3509 | 1 | SROPRT | CHAR 1 |
| 3510 | 10 | SROJN | CHAR 10 |
| 3520 | 10 | SROJL | CHAR 10 |
| 3530 | 10 | SROJRN | CHAR 10 |
| 3540 | 10 | SROJRL | CHAR 10 |
| 3550 | 10 | SROJRA | CHAR 10 |
| 3560 | 10 | SROPFL | CHAR 10 |
| 3570 | 6 | SROPFN | ZONED 6 0 |
| 3576 | 10 | SROPJB | CHAR 10 |
| 3586 | 10 | SROPUN | CHAR 10 |
| 3596 | 6 | SROPJN | CHAR 6 |
| 3602 | 8 | SROPJS | CHAR 8 |
| 3610 | 7 | SROPCD | CHAR 7 |
| 3617 | 6 | SROPCT | CHAR 6 |
| 3623 | 10 | SROPQN | CHAR 10 |
| 3633 | 10 | SROPQL | CHAR 10 |
| 3643 | 10 | SROPUD | CHAR 10 |
| 3653 | 10 | SROPFT | CHAR 10 |
| 3663 | 6 | SROPPG | PACKED 11 0 |
| 3669 | 3 | SROPCP | ZONED 3 0 |
| 3672 | 8 | SROPSZ | PACKED 15 0 |
| 3680 | 7 | SROPXD | CHAR 7 |
| 3687 | 1 | SROPVA | CHAR 1 |
| 3688 | 10 | SROSYN | CHAR 10 |
| 3698 | 2 | SROSYO | ZONED 2 0 |
| 3700 | 32 | SROPSN | CHAR 32 |

### `QSRRST` (4411 bytes)

Record format of `QASRRSTO`, the restore outfile, as IBM i V7R1M0's `DSPFFD` shows it.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | RSRECR | CHAR 1 |
| 1 | 1 | RSOSTA | CHAR 1 |
| 2 | 7 | RSOMSG | CHAR 7 |
| 9 | 1 | RSOSEC | CHAR 1 |
| 10 | 10 | RSNAME | CHAR 10 |
| 20 | 10 | RSMNAM | CHAR 10 |
| 30 | 8 | RSTYPE | CHAR 8 |
| 38 | 10 | RSATTR | CHAR 10 |
| 48 | 15 | RSSIZE | ZONED 15 0 |
| 63 | 10 | RSOOWN | CHAR 10 |
| 73 | 50 | RSOTXT | CHAR 50 |
| 123 | 706 | RSRPLV | VARCHAR 704 |
| 829 | 10 | RSOLIB | CHAR 10 |
| 839 | 2 | RSOASP | ZONED 2 0 |
| 841 | 4 | RSOSEQ | ZONED 4 0 |
| 845 | 17 | RSLABL | CHAR 17 |
| 862 | 13 | RSSVDT | CHAR 13 |
| 875 | 13 | RSRSDT | CHAR 13 |
| 888 | 452 | RSVLID | VARCHAR 450 |
| 1340 | 40 | RSODEV | CHAR 40 |
| 1380 | 10 | RSOSVF | CHAR 10 |
| 1390 | 10 | RSOSFL | CHAR 10 |
| 1400 | 10 | RSOCMD | CHAR 10 |
| 1410 | 8 | RSSYSN | CHAR 8 |
| 1418 | 6 | RSORRL | CHAR 6 |
| 1424 | 6 | RSOSRL | CHAR 6 |
| 1430 | 10 | RSOPGP | CHAR 10 |
| 1440 | 10 | RSOSQ2 | ZONED 10 0 |
| 1450 | 10 | RSONLB | CHAR 10 |
| 1460 | 1 | RSOFMT | CHAR 1 |
| 1461 | 3 | RSOMFN | ZONED 3 0 |
| 1464 | 3 | RSOTMF | ZONED 3 0 |
| 1467 | 10 | RSOMDN | CHAR 10 |
| 1477 | 10 | RSOMDL | CHAR 10 |
| 1487 | 3 | RSOVLC | ZONED 3 0 |
| 1490 | 3 | RSOVLL | ZONED 3 0 |
| 1493 | 2402 | RSOVLD | VARCHAR 2400 |
| 3895 | 258 | RSOOPT | VARCHAR 256 |
| 4153 | 10 | RSOAS1 | CHAR 10 |
| 4163 | 5 | RSOAS2 | ZONED 5 0 |
| 4168 | 1 | RSOPRT | CHAR 1 |
| 4169 | 10 | RSOJN | CHAR 10 |
| 4179 | 10 | RSOJL | CHAR 10 |
| 4189 | 10 | RSOJRN | CHAR 10 |
| 4199 | 10 | RSOJRL | CHAR 10 |
| 4209 | 10 | RSOJRA | CHAR 10 |
| 4219 | 10 | RSOPFL | CHAR 10 |
| 4229 | 6 | RSOPFN | ZONED 6 0 |
| 4235 | 10 | RSOPJB | CHAR 10 |
| 4245 | 10 | RSOPUN | CHAR 10 |
| 4255 | 6 | RSOPJN | CHAR 6 |
| 4261 | 8 | RSOPJS | CHAR 8 |
| 4269 | 7 | RSOPCD | CHAR 7 |
| 4276 | 6 | RSOPCT | CHAR 6 |
| 4282 | 10 | RSOPQN | CHAR 10 |
| 4292 | 10 | RSOPQL | CHAR 10 |
| 4302 | 10 | RSOPUD | CHAR 10 |
| 4312 | 10 | RSOPFT | CHAR 10 |
| 4322 | 6 | RSOPPG | PACKED 11 0 |
| 4328 | 3 | RSOPCP | ZONED 3 0 |
| 4331 | 8 | RSOPSZ | PACKED 15 0 |
| 4339 | 7 | RSOPXD | CHAR 7 |
| 4346 | 7 | RSOPDR | CHAR 7 |
| 4353 | 6 | RSOPTR | CHAR 6 |
| 4359 | 10 | RSOPQR | CHAR 10 |
| 4369 | 10 | RSOPLR | CHAR 10 |
| 4379 | 1 | RSOPVA | CHAR 1 |
| 4380 | 10 | RSOPVS | ZONED 10 0 |
| 4390 | 10 | RSOPVR | ZONED 10 0 |
| 4400 | 1 | RSODFR | CHAR 1 |
| 4401 | 10 | RSODFI | CHAR 10 |
