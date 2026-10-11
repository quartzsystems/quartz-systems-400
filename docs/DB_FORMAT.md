# QS/400 database: on-disk format

How `*FILE` objects, their members, record formats, records and access
paths lie in the object store. Written before the code; `db/tests/spec.rs`
checks the code's `Layout`s against the tables here the way
`store/tests/spec.rs` checks `STORE_FORMAT.md`: every `### \`Name\` (N
bytes)` heading must match a `Layout`, and the `Offset`, `Size` and
`Field` columns must match its fields. Conventions are those of
`STORE_FORMAT.md` §0: little-endian page structures, big-endian tree
keys, CP037 text.

## 1. Objects

A database file is one `*FILE` object (type 0x0013) in a library. Its
attribute area holds `FileAttr`; its byte space holds the definition
area: the record formats as sections, then the member table.

Each member is one `*MBR` object (type 0x00FE) owned by the file: its
header's `library` field is the file's ID and it has no library
directory entry (STORE_FORMAT.md §9, owned objects). The member's byte
space holds the record slots; its header tree roots hold the access
path (slot 0) and the free-slot tree (slot 1). Deleting a file deletes
its members first.

Logical files have no members of their own data: an LF member's `*MBR`
object has an empty byte space and an access path over the based-on
members. `TODO(phase3)`: the based-on, source and select/omit sections
are reserved below and specified with the access path checkpoint.

## 2. The file

### `FileAttr` (128 bytes)

At offset 0 of the attribute area (body offset 1024) of a `*FILE`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400DBF` |
| 8 | 2 | version | 1 |
| 10 | 1 | kind | 0 physical, 1 logical, 2 source physical |
| 11 | 1 | maint | `MAINT`: 0 `*IMMED`, 1 `*DLY`, 2 `*REBLD` |
| 12 | 1 | recover | `RECOVER`: 0 `*NO`, 1 `*AFTIPL`, 2 `*IPL` |
| 13 | 1 | share | `SHARE`: 0 `*NO`, 1 `*YES` |
| 14 | 2 | flags | bit 0 `REUSEDLT(*YES)`, bit 1 `FRCACCPTH(*YES)`, bit 2 `LVLCHK(*NO)`, bit 3 `ALWUPD(*NO)`, bit 4 `ALWDLT(*NO)`, bit 5 `ALLOCATE(*YES)` |
| 16 | 4 | record_len | bytes of the longest record format |
| 20 | 2 | format_count | |
| 22 | 2 | member_count | entries used in the member table |
| 24 | 4 | max_members | `MAXMBRS`, 0 for `*NOMAX` |
| 28 | 4 | size_initial | `SIZE` records, 0 for `*NOMAX` |
| 32 | 4 | size_increment | `SIZE` increment |
| 36 | 4 | size_max_increments | `SIZE` number of increments |
| 40 | 2 | frcratio | `FRCRATIO`, 0 for `*NONE` |
| 42 | 2 | wait_file | `WAITFILE` seconds; 0 `*IMMED`, 0xFFFF `*CLS` |
| 44 | 2 | wait_rcd | `WAITRCD` seconds; 0 `*IMMED`, 0xFFFF `*NOMAX` |
| 46 | 2 | ccsid | file CCSID |
| 48 | 8 | expdate | `EXPDATE` as Unix seconds, 0 for `*NONE` |
| 56 | 8 | def_len | bytes of the byte space in use |
| 64 | 8 | formats_off | byte space offset of the format sections |
| 72 | 8 | formats_len | bytes of the format sections |
| 80 | 8 | members_off | byte space offset of the member table |
| 88 | 4 | member_cap | entries the member table has room for |
| 92 | 4 | reserved | 0 |
| 96 | 32 | reserved2 | 0 |

The member table is `member_cap` × `MemberEntry` at `members_off`, the
first `member_count` in use, in creation order: `*FIRST` is entry 0 and
`*LAST` entry `member_count - 1`. A table that is full is rewritten at
`def_len` with twice the capacity; the old table's bytes are left.

### `MemberEntry` (48 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | name | CP037 |
| 10 | 10 | srctype | source type of a source member, blank otherwise |
| 20 | 2 | flags | reserved, 0 |
| 22 | 10 | reserved | 0 |
| 32 | 16 | object | `ObjectId` of the `*MBR` |

## 3. Record formats

The format sections are a sequence of `Section` headers each followed by
its body. A `FORMAT` section is followed by its `FIELD` sections in
buffer order and then its `KEY` sections in key order; the next `FORMAT`
section starts the next format.

### `Section` (8 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | tag | see below |
| 2 | 2 | reserved | 0 |
| 4 | 4 | len | bytes of the body that follows |

| Tag | Body | Meaning |
|---|---|---|
| 1 | `FormatEntry` | a record format |
| 2 | `FieldEntry` | a field of the preceding format |
| 3 | `KeyEntry` | a key field of the preceding format |
| 4 | reserved | `TODO(phase3)` based-on file of a logical format |
| 5 | reserved | `TODO(phase3)` field source of a logical field |
| 6 | reserved | `TODO(phase3)` select/omit |
| 7 | reserved | `TODO(phase3)` join |

### `FormatEntry` (96 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | name | CP037 |
| 10 | 13 | level_id | §5, ASCII hex |
| 23 | 1 | reserved | 0 |
| 24 | 2 | field_count | |
| 26 | 2 | key_count | 0 for arrival sequence |
| 28 | 4 | record_len | bytes of data per record |
| 32 | 2 | null_map_len | bytes of the null map per record, 0 when no field is null-capable |
| 34 | 2 | flags | bit 0 `UNIQUE`, bit 1 `*EXCNULL`, bits 2-3 duplicate order: 0 none, 1 `FIFO`, 2 `LIFO`, 3 `FCFO` |
| 36 | 2 | key_len | bytes of the encoded key, §6 |
| 38 | 8 | reserved2 | 0 |
| 46 | 50 | text | `TEXT`, CP037 |

### `FieldEntry` (256 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | name | CP037 |
| 10 | 1 | data_type | the DDS letter, ASCII: `A H G O S P B F L T Z` |
| 11 | 1 | decimals | |
| 12 | 2 | flags | bit 0 `ALWNULL`, bit 1 `VARLEN`, bit 2 usage `B` |
| 14 | 4 | length | characters or digits |
| 18 | 4 | byte_len | bytes in the record, `VARLEN` prefix included |
| 22 | 4 | offset | byte offset in the record |
| 26 | 2 | ccsid | 0 for the file's |
| 28 | 1 | date_fmt | `DATFMT`: 0 `*ISO`, 1 `*USA`, 2 `*EUR`, 3 `*JIS`, 4 `*MDY`, 5 `*DMY`, 6 `*YMD`, 7 `*JUL`; 0 when not a date |
| 29 | 1 | date_sep | ASCII separator in effect, 0 when not a date |
| 30 | 1 | time_fmt | `TIMFMT`: 0 `*HMS`, 1 `*ISO`, 2 `*USA`, 3 `*EUR`, 4 `*JIS`; 0 when not a time |
| 31 | 1 | time_sep | ASCII separator in effect, 0 when not a time |
| 32 | 1 | precision | `FLTPCN`: 0 `*SINGLE`, 1 `*DOUBLE` |
| 33 | 1 | edit_code | `EDTCDE` letter, ASCII, 0 for none |
| 34 | 2 | varlen_alloc | `VARLEN` allocated length, 0 for none |
| 36 | 2 | null_index | bit in the null map, 0xFFFF when not null-capable |
| 38 | 50 | text | `TEXT`, CP037 |
| 88 | 30 | alias | `ALIAS`, CP037 |
| 118 | 60 | colhdg | `COLHDG`, three CP037 lines of 20 |
| 178 | 1 | dft_kind | `DFT`: 0 none, 1 `*NULL`, 2 quoted, 3 hexadecimal, 4 numeric |
| 179 | 1 | dft_len | bytes used in `dft` |
| 180 | 64 | dft | the default: CP037 text, raw hex bytes or ASCII digits; longer defaults are cut |
| 244 | 12 | reserved | 0 |

Validity keywords (`CHECK`, `VALUES`, `RANGE`, `COMP`) and `EDTWRD` are
not kept in the file. `TODO(phase3)`: DSPFFD shows them when kept.

### `KeyEntry` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | field | index of the field in the format |
| 2 | 2 | flags | bit 0 `DESCEND`, bits 1-3 transform: 0 `SIGNED`, 1 `UNSIGNED`, 2 `ABSVAL`, 3 `ZONE`, 4 `DIGIT`; bit 4 `NOALTSEQ` |
| 4 | 2 | key_off | offset of this field's part in the encoded key |
| 6 | 2 | key_len | bytes of this field's part, §6 |
| 8 | 8 | reserved | 0 |

## 4. The member

### `MbrAttr` (128 bytes)

At offset 0 of the attribute area of a `*MBR`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400MBR` |
| 8 | 2 | version | 1 |
| 10 | 2 | flags | bit 0 access path invalid (rebuild before use), bit 1 `REUSEDLT(*YES)`: deleted slots are reused |
| 12 | 4 | slot_len | bytes per record slot |
| 16 | 8 | record_count | active records |
| 24 | 8 | deleted_count | deleted records still holding a slot |
| 32 | 8 | slots | slots used; the next new record is RRN `slots + 1` unless a slot is reused |
| 40 | 8 | max_records | from `SIZE`, 0 for `*NOMAX` |
| 48 | 8 | increment_slots | slots added when the space grows |
| 56 | 8 | path_entries | entries in the access path |
| 64 | 2 | key_len | encoded key bytes without the RRN tail, 0 for arrival sequence |
| 66 | 2 | format | index of the format the access path serves |
| 68 | 2 | null_map_len | bytes of null map per record |
| 70 | 2 | reserved | 0 |
| 72 | 16 | file | `ObjectId` of the owning file |
| 88 | 8 | last_save | Unix seconds, 0 for never |
| 96 | 8 | last_restore | Unix seconds, 0 for never |
| 104 | 24 | reserved2 | 0 |

Header tree roots: slot 0 the access path (§6), slot 1 the free-slot tree
(`REUSEDLT(*YES)`): keys are RRNs of deleted slots, values empty.

### `RecordSlot` (variable)

The member's byte space is `slots` slots of `slot_len` bytes from offset
0; slot `i` holds RRN `i + 1`. Slots cross page boundaries. `slot_len`
is `1 + record_len + null_map_len`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | status | 0 never written, 1 active, 2 deleted |
| 1 | record_len | record | the record, every field at its format offset; a `VARLEN` field is its 2-byte length then its bytes, blank padded to the maximum |
| 1 + record_len | null_map_len | null_map | bit `null_index` of each null-capable field set when it is null |

`TODO(perf)`: a `VARLEN` field is stored at its maximum length; the
`ALLOCATE` length and an overflow extent are not used.

## 5. Format level identifier

`fmt_level_id(fields)` is the first 52 bits of the SHA-256 of the
format's fields, each written as 20 bytes in buffer order: `name` (10,
CP037), the DDS type letter (1, ASCII), `length` (4, big-endian),
`decimals` (1), `ccsid` (2, big-endian, 0 when the file's), null-capable
(1) and `VARLEN` (1). The 52 bits are the 13 uppercase hexadecimal
digits the level check compares and `DSPFD` shows. The format name and
text are not part of it. Every path that makes a format (DDS, SQL DDL,
later RPG) calls the one function; the algorithm does not change within
V1R.

## 6. Access path keys

An access path is a B+tree owned by the member with `key_len + 8`-byte
keys and empty values: the encoded key followed by the RRN, big-endian,
so equal keys keep arrival order and every key is unique in the tree.
Byte order of the encoded key is the key order DDS asks for:

- Character, hexadecimal, graphic and DBCS-open fields: the bytes as
  stored, a `VARLEN` field at its maximum length. `ZONE` keeps the high
  nibble of each byte and `DIGIT` the low one; `ALTSEQ` is applied
  before the comparison (`TODO(phase3)`).
- Zoned, packed and binary fields: 1 byte of sign then `length` digit
  bytes. `SIGNED` (the default): 0x01 for negative, 0x02 for positive or
  zero; positive digits as `0x30 + d`, negative digits complemented as
  `0x39 - d` so larger magnitudes sort lower. `UNSIGNED` and `ABSVAL`:
  sign byte 0x02 and the digits of the magnitude.
- Float: 8 bytes, the IEEE double with the sign bit flipped when
  positive and every bit flipped when negative.
- Date: 8 bytes `yyyymmdd`; time: 6 bytes `hhmmss`; timestamp: its 26
  stored bytes.
- A null-capable field is preceded by one byte: 0x00 when it holds a
  value, 0xFF when null, so null sorts high.
- `DESCEND`: every byte of the field's part is complemented.

`KeyEntry.key_len` is the part's length by these rules; `key_len` of the
format and member is their sum. The sum is at most 1016 bytes (the
store's 1024-byte key less the RRN); DDS allows 2000 and the compiler
still says so, so a file whose key is longer than 1016 bytes is refused
at create (`DB_OPEN_QUESTIONS.md`).

`TODO(phase3)`: multi-format and join paths, select/omit, `MAINT`
states and rebuilds are specified with the access path checkpoint.
