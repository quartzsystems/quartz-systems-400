# QS/400 database: on-disk format

How `*FILE` objects, their members, record formats, records and access
paths lie in the object store. Written before the code; `database/db/tests/spec.rs`
checks the code's `Layout`s against the tables here the way
`storage/store/tests/spec.rs` checks `STORE_FORMAT.md`: every `### \`Name\` (N
bytes)` heading must match a `Layout`, and the `Offset`, `Size` and
`Field` columns must match its fields. Conventions are those of
`STORE_FORMAT.md` §0: little-endian page structures, big-endian tree
keys, CP037 text.

## 1. Objects

A database file is one `*FILE` object (type 0x0013) in a library. Its
attribute area holds `FileAttr`; its byte space holds the definition
area: the record formats as sections, the member table, and for a
physical file the dependents table.

Each member is one `*MBR` object (type 0x00FE) owned by the file: its
header's `library` field is the file's ID and it has no library
directory entry (STORE_FORMAT.md §9, owned objects). A physical member's
byte space holds the record slots; its header tree roots hold the access
path (slot 0) and the free-slot tree (slot 1). Deleting a file deletes
its members first.

A logical member holds no records: its byte space is empty and its
access path (header tree root slot 0) is over the records of the
based-on physical members named in its attribute area. Every physical
member keeps, in its file's dependents table, the logical members whose
paths it must maintain.

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
| 14 | 2 | flags | bit 0 `REUSEDLT(*YES)`, bit 1 `FRCACCPTH(*YES)`, bit 2 `LVLCHK(*NO)`, bit 3 `ALWUPD(*NO)`, bit 4 `ALWDLT(*NO)`, bit 5 `ALLOCATE(*YES)`, bit 6 `DYNSLT`, bit 7 `JDFTVAL` |
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
| 92 | 4 | dependent_count | entries used in the dependents table |
| 96 | 8 | dependents_off | byte space offset of the dependents table, 0 for none |
| 104 | 4 | dependent_cap | entries the dependents table has room for |
| 108 | 16 | journal | `ObjectId` of the journal the file is journaled to, none when zero |
| 124 | 2 | jrn_flags | bit 0 `IMAGES(*BOTH)`, bit 1 `OMTJRNE(*OPNCLO)` |
| 126 | 2 | reserved | 0 |

The member table is `member_cap` × `MemberEntry` at `members_off`, the
first `member_count` in use, in creation order: `*FIRST` is entry 0 and
`*LAST` entry `member_count - 1`. A table that is full is rewritten at
`def_len` with twice the capacity; the old table's bytes are left. The
dependents table at `dependents_off` works the same way.

### `MemberEntry` (48 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | name | CP037 |
| 10 | 10 | srctype | source type of a source member, blank otherwise |
| 20 | 2 | flags | reserved, 0 |
| 22 | 10 | reserved | 0 |
| 32 | 16 | object | `ObjectId` of the `*MBR` |

### `DependentEntry` (48 bytes)

A logical member whose access path a physical member of this file
maintains. Added by `ADDLFM` (and `CRTLF` with a member), removed by
`RMVM` and `DLTF` of the logical file.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | lf_file | `ObjectId` of the logical file |
| 16 | 16 | lf_member | `ObjectId` of its member |
| 32 | 16 | base_member | `ObjectId` of the physical member it depends on |

## 3. Record formats

The format sections are a sequence of `Section` headers each followed by
its body. A `FORMAT` section is followed by its `FIELD` sections in
buffer order, its `KEY` sections in key order, and for a logical format
its `BASEDON` sections in file order, one `SOURCE` per field in buffer
order, its `SELOMIT` groups in source order and its `JOIN` sections in
source order; the next `FORMAT` section starts the next format.

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
| 4 | `BasedOnEntry` | a based-on file of a logical format |
| 5 | `SourceEntry` | where a logical field's value comes from |
| 6 | `SelOmitGroup` | a select or omit group of a logical format |
| 7 | `JoinEntry` | a join specification of a join logical format |

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
| 34 | 2 | flags | bit 0 `UNIQUE`, bit 1 `*EXCNULL`, bits 2-3 duplicate order: 0 none, 1 `FIFO`, 2 `LIFO`, 3 `FCFO`; bit 4 join format |
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
| 178 | 1 | dft_kind | `DFT`: 0 none, 1 `*NULL`, 2 quoted, 3 hexadecimal, 4 numeric, 5 an SQL expression evaluated at insert (a special register) |
| 179 | 1 | dft_len | bytes used in `dft` |
| 180 | 64 | dft | the default: CP037 text, raw hex bytes, ASCII digits or ASCII SQL text; longer defaults are cut |
| 244 | 1 | identity | bit 0 an SQL identity column, bit 1 `GENERATED ALWAYS`, bit 2 `CYCLE` |
| 245 | 4 | id_start | `START WITH`, signed |
| 249 | 4 | id_incr | `INCREMENT BY`, signed |
| 253 | 3 | reserved | 0 |

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

### `BasedOnEntry` (32 bytes)

`PFILE` or `JFILE` file `index` of a logical format.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | file | `ObjectId` of the physical file |
| 16 | 10 | format | its record format used, CP037 |
| 26 | 2 | index | position in the `PFILE`/`JFILE` list, from 0 |
| 28 | 4 | reserved | 0 |

### `SourceEntry` (variable)

One per field of a logical format, in buffer order. `names` are the
based-on fields: one for a plain or renamed field or an `SST`, the parts
in order for a `CONCAT`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | kind | 0 based-on field, 1 `CONCAT`, 2 `SST` |
| 1 | 1 | file | based-on file index |
| 2 | 2 | count | names that follow, 1 unless `CONCAT` |
| 4 | 2 | start | `SST` first position, 1-based; 0 otherwise |
| 6 | 2 | length | `SST` length; 0 otherwise |
| 8 | count × 10 | names | CP037 field names |

### `SelOmitGroup` (8 bytes)

An `S` or `O` statement and its AND conditions; `cond_count` ×
`SelOmitCondition` follow in the same section body.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | select | 1 select, 0 omit |
| 1 | 1 | all | 1 for `ALL` |
| 2 | 2 | cond_count | |
| 4 | 4 | reserved | 0 |

### `SelOmitCondition` (8 bytes)

Followed by `value_count` × `SelOmitValue`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | field | index of the field in the logical format |
| 2 | 1 | test | 0 `VALUES`, 1 `RANGE`, 2 `COMP` |
| 3 | 1 | op | `COMP` operator: 0 `EQ`, 1 `NE`, 2 `LT`, 3 `NL`, 4 `GT`, 5 `NG`, 6 `LE`, 7 `GE`; 0 otherwise |
| 4 | 2 | value_count | `RANGE` has 2, `COMP` 1 |
| 6 | 2 | reserved | 0 |

### `SelOmitValue` (variable)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | kind | 0 unquoted (a number or special value, ASCII), 1 quoted (CP037), 2 hexadecimal |
| 1 | 1 | reserved | 0 |
| 2 | 2 | len | bytes that follow |
| 4 | len | bytes | the literal as written |

### `JoinEntry` (variable)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | from | based-on file index |
| 1 | 1 | to | based-on file index |
| 2 | 2 | pair_count | `JFLD` pairs that follow |
| 4 | 1 | dupseq | 1 when `JDUPSEQ` is given |
| 5 | 1 | dupseq_descend | 1 for `*DESCEND` |
| 6 | 2 | reserved | 0 |
| 8 | 10 | dupseq_field | CP037, blank without `JDUPSEQ` |
| 18 | pair_count × 20 | pairs | from-file field name then to-file field name, CP037 |

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
| 64 | 2 | key_len | encoded key bytes without the format tag and RRN tail, 0 for arrival sequence |
| 66 | 2 | format | index of the format the access path serves, 0xFFFF for a multi-format path |
| 68 | 2 | null_map_len | bytes of null map per record |
| 70 | 2 | base_count | logical member: based-on members that follow the attributes |
| 72 | 16 | file | `ObjectId` of the owning file |
| 88 | 8 | last_save | Unix seconds, 0 for never |
| 96 | 8 | last_restore | Unix seconds, 0 for never |
| 104 | 8 | identity_next | the next value of the format's identity column, signed; 0 when it has none |
| 112 | 16 | reserved2 | 0 |

A logical member's based-on physical members follow at offset 128 of
the attribute area, `base_count` × `ObjectId`, one per format of the
logical file in format order (every format of a `PFILE` list uses its
first file). Header tree roots: slot 0 the access path (§6), slot 1 the
free-slot tree (`REUSEDLT(*YES)`): keys are RRNs of deleted slots, values
empty.

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

## 6. Access paths

An access path is a B+tree owned by the member with `PathKey` keys and
empty values: the encoded key, for a multi-format path the format
number, then the RRN, big-endian, so equal keys keep arrival order and
every entry is unique in the tree.

### `PathKey` (variable)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | key_len | key | the encoded key, a shorter format's key padded with 0x00 in a multi-format path |
| key_len | 2 | format | format number in a multi-format path; absent in a single-format path |
| key_len + 2 | 8 | rrn | relative record number in the based-on member |

Byte order of the encoded key is the key order DDS asks for:

- Character, hexadecimal, graphic and DBCS-open fields: the bytes as
  stored, a `VARLEN` field at its maximum length blank padded. `ZONE`
  keeps the high nibble of each byte and `DIGIT` the low one. `ALTSEQ`
  is not applied (`DB_OPEN_QUESTIONS.md`).
- Zoned, packed and binary fields: 1 byte of sign then `length` digit
  bytes. `SIGNED` (the default): 0x01 for negative, 0x02 for positive or
  zero; positive digits as `0x30 + d`, negative digits complemented as
  `0x39 - d` so larger magnitudes sort lower. `UNSIGNED` and `ABSVAL`:
  sign byte 0x02 and the digits of the magnitude.
- Float: 8 bytes, the IEEE double with the sign bit flipped when
  positive and every bit flipped when negative.
- Date: 8 ASCII digits `yyyymmdd` (`*JUL` as `yyyy0ddd`), a two-digit
  year read as 1940 to 2039; time: 6 ASCII digits `hhmmss` on the
  24-hour clock; timestamp: its 26 stored bytes.
- A null-capable field is preceded by one byte: 0x00 when it holds a
  value, 0xFF when null, so null sorts high; a null field's digits or
  bytes are zeros.
- `DESCEND`: every byte of the field's part, the null byte included, is
  complemented.

`KeyEntry.key_len` is the part's length by these rules; `key_len` of the
format is their sum, and of a multi-format member the largest format's.
The sum is at most 2000 bytes, DDS's own limit; with the tail the tree
key is at most 2010 of the store's 2048.

**Maintenance.** With `MAINT(*IMMED)` every write, update and delete of
a physical record changes the member's own path and every dependent
logical member's path in the same transaction, after `UNIQUE` is
checked on each path that asks for it (`*INCNULL` compares nulls equal;
`*EXCNULL` never counts a key with a null field as a duplicate). With
`*DLY` or `*REBLD` the dependent path is marked invalid instead (flag
bit 0 of its `MbrAttr`) and is rebuilt by bulk load at the next open
(`*DLY`) or when a keyed open asks for it (`*REBLD`); a rebuild reports
progress per based-on record.

**Logical records.** A logical format's record is derived from the
based-on record field by field: a based-on field's value converted to
the logical field's type and length, a `CONCAT` as the parts' bytes in
order (zoned parts keep the sign of the last), an `SST` as the slice;
a null based-on field makes a null logical field. Select/omit groups are
tried in order: a group whose conditions all hold (or `ALL`) decides;
when none applies the record is selected if the last group is an omit
and omitted if it is a select. Without `DYNSLT` only selected records
enter the path; with it every record does and the test is applied on
read. Join formats take their key and their path from the primary file,
one entry per primary record; a read expands the entry into one record
per matching secondary record, the secondaries read then (through their
own path when it leads with the `JFLD` fields), `JDFTVAL` keeping an
unmatched primary record with zero and blank secondary fields. A
select/omit test on a secondary field runs at read time.

## 7. Journals

A journal is a `*JRN` object (type 0x0018) and a receiver a `*JRNRCV`
(type 0x0019), both ordinary library objects in the ASP of the data they
journal. The journal's attribute area holds `JrnAttr`; its byte space
holds the receiver directory (`receiver_cap` × `RcvDirEntry` at
`dir_off`) and the journaled objects (`object_cap` × `JrnObjEntry` at
`obj_off`). Both tables grow like the member table: rewritten at
`def_len` with twice the capacity.

A receiver's attribute area holds `RcvAttr`; its byte space is the
entries, back to back, `bytes_used` of it in use. An entry is never
changed once written. Receivers form a chain through `prev` and `next`;
the directory lists them oldest first. Entries are numbered once per
journal (`next_seq`) and a receiver holds a contiguous range
(`first_seq` to `last_seq`).

Every entry is written in the transaction that made the change it
describes, so the journal and the data commit together.

### `JrnAttr` (128 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400JRN` |
| 8 | 2 | version | 1 |
| 10 | 2 | flags | bit 0 `MNGRCV(*SYSTEM)`, bit 1 `DLTRCV(*YES)`, bit 2 `MINENTDTA`, bit 3 `FIXLENDTA` |
| 12 | 2 | rcvsizopt | `RCVSIZOPT` flags, kept |
| 14 | 2 | reserved | 0 |
| 16 | 8 | next_seq | sequence number of the next entry |
| 24 | 16 | attached | `ObjectId` of the attached receiver, none when zero |
| 40 | 4 | receiver_count | entries used in the receiver directory |
| 44 | 4 | receiver_cap | entries the directory has room for |
| 48 | 8 | dir_off | byte space offset of the directory |
| 56 | 4 | object_count | journaled objects |
| 60 | 4 | object_cap | entries the object table has room for |
| 64 | 8 | obj_off | byte space offset of the object table |
| 72 | 8 | def_len | bytes of the byte space in use |
| 80 | 48 | reserved2 | 0 |

### `RcvDirEntry` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | receiver | `ObjectId` of the `*JRNRCV` |
| 16 | 1 | status | 0 empty, 1 attached, 2 online, 3 saved |
| 17 | 1 | flags | bit 0 `*PARTIAL`: the chain is broken before this receiver |
| 18 | 6 | reserved | 0 |
| 24 | 8 | attached | Unix seconds |

### `JrnObjEntry` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | object | `ObjectId` of the journaled object |
| 16 | 2 | object_type | its type code |
| 18 | 1 | images | 1 `IMAGES(*AFTER)`, 2 `IMAGES(*BOTH)` |
| 19 | 1 | flags | bit 0 `OMTJRNE(*OPNCLO)` |
| 20 | 4 | reserved | 0 |
| 24 | 8 | started | journaling started, Unix seconds |

### `RcvAttr` (128 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400RCV` |
| 8 | 2 | version | 1 |
| 10 | 1 | status | 0 empty, 1 attached, 2 online, 3 saved |
| 11 | 1 | flags | reserved, 0 |
| 12 | 4 | reserved | 0 |
| 16 | 16 | journal | `ObjectId` of the journal, none before the first attach |
| 32 | 8 | first_seq | first sequence number held, 0 when empty |
| 40 | 8 | last_seq | last sequence number held |
| 48 | 8 | entry_count | entries held |
| 56 | 8 | bytes_used | bytes of entries |
| 64 | 4 | threshold_kb | `THRESHOLD` in KiB, 0 for `*NONE` |
| 68 | 4 | reserved2 | 0 |
| 72 | 8 | attached | Unix seconds |
| 80 | 8 | detached | Unix seconds, 0 while attached |
| 88 | 16 | prev | previous receiver in the chain, none when zero |
| 104 | 16 | next | next receiver in the chain |
| 120 | 8 | reserved3 | 0 |

### `EntryHeader` (128 bytes)

Followed by `data_len` bytes of entry-specific data. The fields are
those IBM's `QjoRetrieveJournalEntries` returns; names in CP037.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | len | bytes of the whole entry, header included |
| 4 | 8 | seq | sequence number |
| 12 | 1 | code | journal code: `J`, `F`, `R`, `C`, `U`, `T` |
| 13 | 2 | type | entry type, two letters |
| 15 | 1 | flags | bit 0 in a commit cycle, bit 1 minimized image, bit 2 written for a rollback, bit 3 written by `APYJRNCHG`/`RMVJRNCHG` |
| 16 | 8 | time | Unix seconds |
| 24 | 4 | micros | microseconds within the second |
| 28 | 4 | job_number | |
| 32 | 10 | job_name | |
| 42 | 10 | job_user | |
| 52 | 10 | program | |
| 62 | 10 | library | of the object, blank when none |
| 72 | 10 | object | |
| 82 | 10 | member | blank when none |
| 92 | 2 | object_type | type code |
| 94 | 2 | reserved | 0 |
| 96 | 8 | count | count or relative record number |
| 104 | 8 | commit_cycle | 0 outside commitment control |
| 112 | 4 | data_len | |
| 116 | 12 | reserved2 | 0 |

Record-level entries (`R`) carry a record image: a 4-byte record length,
a 2-byte null map length, the record, the null map. `R PT` holds the
record written, `R UB` the record before an update (under
`IMAGES(*BOTH)`), `R UP` the record after, `R DL` the record deleted.
`J PR` and `J NR` name the previous and next receiver in `object`.
File-level entries (`F`) carry no data.
