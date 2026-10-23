# Display files

The `dspf` crate (`ui/dspf`) implements this document: the DDS
display-file compiler behind `CRTDSPF`, the model it produces, the
record buffers a program exchanges with a display file and the payload
the `*FILE` object keeps. It is a library with no runtime: nothing here
writes a 5250 data stream or touches the store. `ui/dspf/tests/spec.rs`
compares the payload tables and the keyword matrix below with the code.
Every text is ours; the DDS columns, the keyword names and parameters,
the display sizes and the buffer rules are the compatibility interface.

## 1. Source and conditioning

The source form, the keyword grammar (`KEYWORD(parm ...)`, quoted
strings, `X'..'`, nested groups, `+`/`-` continuation) and the
diagnostics come from the `dds` crate of Phase 3. The display file adds
the columns the database compilers reject:

| Positions | Use |
|---|---|
| 7 | `A` or `O`: continue the condition of the line before |
| 8-16 | up to three option indicators, each `N` or blank then `01`-`99`; or `*DSn` |
| 17 | `R` starts a record format; blank is a field or constant |
| 19-28 | the field name; blank for a constant |
| 29 | `R`: take the shape from the referenced field |
| 30-34, 35, 36-37 | length, type and decimal positions |
| 38 | usage: blank or `O`, `I`, `B`, `H`, `M`, `P` |
| 39-41, 42-44 | row and column, or `+n` after the previous field |
| 45-80 | keywords and `'constants'` |

A condition group runs from a line with position 7 blank over the
`A` and `O` lines that follow it. The indicators of one line must all
hold; an `A` line adds to the group; an `O` line starts another group,
and any group holding is enough. The field or keyword on any line of a
group is conditioned by the whole group, so indicators on the line
that names a field condition the field and every keyword on that line.
`*DSn` in positions 8-16 conditions on a display size instead; a line
with `*DSn` and only a location places the field before it on that
size (an alternate location).

Data types: `A N X W I D M E J O` are character (`N` with decimal
positions is zoned), `G` graphic, `S Y` zoned, `P` packed, `B` binary,
`F` float, `L T Z` date, time and timestamp. Blank is `A`, or `S` when
decimal positions are given.

## 2. The model

`compile_dspf(&Source, &dyn RefResolver, &CompileOpts) -> Compilation<DisplayFile>`
is the entry; `Compilation` carries the definition (when nothing at
severity 20 or more was found), every diagnostic and the listing.

`DisplayFile` holds the file attributes, the records in source order and
the source itself:

- `FileAttrs`: `DSPSIZ` as one or two `DisplaySize`s (the first primary),
  `INDARA`, `INDTXT`, `CHGINPDFT`, `REF`, `MSGLOC`, the flags `RSTDSP
  USRDSPMGT CSRINPONLY DSPRL ERRSFL`, `ALTHELP`/`ALTPAGEUP`/`ALTPAGEDWN`,
  the file-level keys (`CmdKey`: key, response indicator, text,
  condition), `CCSID`, and every accepted keyword as a `KeywordUse`
  (name, parameters, condition, line).
- `Record`: name, `TEXT`, kind (`Normal`, `Subfile`, `SubfileControl`,
  `MenuBar`, `PullDown`), the `Subfile` a control record drives (the
  `SFL` record, size and page per display size, `SFLLIN`, `SFLEND`, the
  page's lines), the `Window` (origin and usable size), the record's
  keys, `CLRL`, `SLNO`, `SFLMSGRCD`, the response indicators record
  keywords set, the fields, the `RecordLayout` and every accepted
  keyword.
- `Field`: name or `Constant` (`'text'`, `DFT`, `DATE`, `TIME`, `USER`,
  `SYSNAME`, `MSGCON`), `FieldType` (the letter and its buffer class),
  length, decimals, `Usage`, the position on each display size, the
  screen length, the buffer bytes, the field's condition, `DSPATR` and
  `COLOR` with their conditions, `EDTCDE`/`EDTWRD`, `CHECK`, `VALUES`/
  `RANGE`/`COMP`, `ERRMSG`/`ERRMSGID`, `DFT`, `TEXT`, the reference the
  shape came from, `DATFMT`/`DATSEP`/`TIMFMT`/`TIMSEP`, the subfile role
  (`SFLRCDNBR`, `SFLPGMQ`, `SFLMSGKEY`, `SFLROLVAL`), the response
  indicators it sets and every accepted keyword.

A keyword the table marks "accepted and stored" appears only in the
`keywords` list of its level, with its condition; the runtime of a later
phase reads it from there. `DisplayFile::indicator_usage()` tells where
each indicator is used; `indicator_area_len()` is 99 with `INDARA`.
`encode`/`decode` are the payload; `kw::TABLE` is the matrix below.

## 3. Keywords

Status: *supported* means parsed into the model and checked by its
rules; *accepted and stored* means the level and parameter count are
checked, the keyword is kept with its conditioning and a severity-00
note (`DDS6210`) says so; *rejected* is refused with `DDS6202`. Anything
not in the table is `DDS0209`, unknown. `CAnn` and `CFnn` stand for the
48 command keys. Option indicators are refused (`DDS6207`) on the
keywords IBM documents as unconditionable: `DSPSIZ INDARA INDTXT REF
REFFLD TEXT ALIAS CCSID SFL SFLCTL SFLSIZ SFLPAG SFLLIN WINDOW DFT
EDTCDE EDTWRD` and the other shape keywords; `*DSn` conditions are
allowed on every keyword.

| Keyword | Levels | Status | Notes |
|---|---|---|---|
| `ALARM` | record | supported |  |
| `ALIAS` | field | accepted and stored |  |
| `ALTHELP` | file | supported | CAnn, CA01 by default |
| `ALTPAGEDWN` | file | supported | CFnn, CF08 by default |
| `ALTPAGEUP` | file | supported | CFnn, CF07 by default |
| `ASSUME` | record | supported |  |
| `AUTO` | field | supported | RA RAB RAZ; input-capable fields |
| `BLANKS` | field | supported | response indicator; input-capable fields |
| `BLINK` | record | supported |  |
| `CAnn` | file, record | supported | CA01-CA24; response indicator and text; same number as a CFnn is refused |
| `CCSID` | file, field | accepted and stored |  |
| `CFnn` | file, record | supported | CF01-CF24; response indicator and text |
| `CHANGE` | record, field | supported | response indicator and text |
| `CHCACCEL` | field | accepted and stored |  |
| `CHCAVAIL` | file, record, field | accepted and stored |  |
| `CHCCTL` | field | accepted and stored |  |
| `CHCSLT` | file, record, field | accepted and stored |  |
| `CHCUNAVAIL` | file, record, field | accepted and stored |  |
| `CHECK` | field | supported | AB ER FE LC M10 M10F M11 M11F MF ME RB RL RLTB RZ VN VNE; input-capable fields |
| `CHGINPDFT` | file, record, field | supported | BL CS HI RI UL ND FE LC ME MF |
| `CHOICE` | field | accepted and stored |  |
| `CHRID` | field | accepted and stored |  |
| `CLEAR` | file, record | supported |  |
| `CLRL` | record | supported | n, *NO, *END, *ALL; no option indicators |
| `CMP` | field | supported | as COMP |
| `CNTFLD` | field | accepted and stored |  |
| `COLOR` | field | supported | BLU GRN PNK RED TRQ WHT YLW |
| `COMP` | field | supported | EQ NE LT NL GT NG LE GE and a literal; input-capable fields |
| `CSRINPONLY` | file, record | supported |  |
| `CSRLOC` | record | supported | two hidden zoned 3,0 fields |
| `DATE` | field | supported | *SYS/*JOB, *Y/*YY; 6 or 8 positions, plus 2 with EDTCDE(Y) |
| `DATFMT` | field | supported | *ISO *USA *EUR *JIS *MDY *DMY *YMD *JUL; sets the length of an L field |
| `DATSEP` | field | supported | one character or *JOB |
| `DFT` | field | supported | a constant without a name; the default of a named field, checked against its length |
| `DLTCHK` | field | accepted and stored |  |
| `DLTEDT` | field | accepted and stored |  |
| `DSPATR` | field | supported | BL CS HI ND PC PR RI UL MDT OID SP or &field; PR MDT OID SP need I or B |
| `DSPMOD` | record, field | accepted and stored |  |
| `DSPRL` | file, record, field | supported |  |
| `DSPSIZ` | file | supported | 24 80 and 27 132, named *DS3 and *DS4; one or two sizes |
| `DUP` | field | supported | response indicator; input-capable fields |
| `EDTCDE` | field | supported | the database edit codes; the screen length comes from qdecimal; Y on DATE |
| `EDTMSK` | field | accepted and stored |  |
| `EDTWRD` | field | supported | the word is the screen length |
| `ENTFLDATR` | file, record, field | accepted and stored |  |
| `ERASE` | record | supported | the named records must exist |
| `ERASEINP` | record | supported |  |
| `ERRMSG` | field | supported | text and response indicator |
| `ERRMSGID` | field | supported | identifier, message file, indicator, &field |
| `ERRSFL` | file | supported |  |
| `FLDCSRPRG` | field | accepted and stored |  |
| `FLTFIXDEC` | field | accepted and stored |  |
| `FLTPCN` | field | accepted and stored |  |
| `FRCDTA` | record | supported |  |
| `GETRETAIN` | record | accepted and stored |  |
| `HELP` | file, record | supported |  |
| `HLPARA` | record | accepted and stored |  |
| `HLPCLR` | file, record | accepted and stored |  |
| `HLPCMDKEY` | file, record | accepted and stored |  |
| `HLPPNLGRP` | file, record | accepted and stored |  |
| `HLPRCD` | record | accepted and stored |  |
| `HLPRTN` | file, record | accepted and stored |  |
| `HLPSCHIDX` | file | accepted and stored |  |
| `HLPSEQ` | file, record | accepted and stored |  |
| `HLPSHELF` | file | accepted and stored |  |
| `HLPTITLE` | file, record | accepted and stored |  |
| `HOME` | file, record | supported |  |
| `HTML` | record, field | rejected | no browser output in this release |
| `IGCALTTYP` | file | rejected | no DBCS support |
| `IGCCNV` | file | rejected | no DBCS support |
| `INDARA` | file | supported | a separate 99-byte indicator area instead of the buffer prefix |
| `INDTXT` | file, record, field | supported | shown in the indicator usage |
| `INVITE` | file, record | supported |  |
| `INZINP` | record | supported |  |
| `INZRCD` | record | supported |  |
| `KEEP` | record | supported |  |
| `LOCK` | record | supported |  |
| `LOGINP` | record | accepted and stored |  |
| `LOGOUT` | record | accepted and stored |  |
| `MAPVAL` | field | accepted and stored |  |
| `MDTOFF` | record | accepted and stored |  |
| `MLTCHCFLD` | field | accepted and stored |  |
| `MNUBAR` | record | supported | the record needs a field with MNUBARCHC |
| `MNUBARCHC` | field | supported | the pull-down record must exist |
| `MNUBARDSP` | record | supported | the menu bar record must exist |
| `MNUBARSEP` | file, record | accepted and stored |  |
| `MNUBARSW` | file, record | accepted and stored |  |
| `MNUCNL` | file, record | supported | CAnn or CFnn and a response indicator |
| `MOUBTN` | field | accepted and stored |  |
| `MSGCON` | field | supported | a constant of the given length from a message description |
| `MSGID` | field | supported | fields with usage M |
| `MSGLOC` | file, record | supported | within the smallest display size |
| `NOCCSID` | field | accepted and stored |  |
| `OPENPRT` | file | accepted and stored |  |
| `OVERLAY` | record | supported |  |
| `OVRATR` | record, field | supported |  |
| `OVRDTA` | record, field | supported |  |
| `PAGEDOWN` | file, record | supported |  |
| `PAGEUP` | file, record | supported |  |
| `PRINT` | file, record | supported |  |
| `PROTECT` | record | supported |  |
| `PSHBTNCHC` | field | accepted and stored |  |
| `PSHBTNFLD` | field | accepted and stored |  |
| `PULLDOWN` | record | supported | field positions are relative to the pull-down window |
| `PULLINPUT` | record | accepted and stored |  |
| `PUTOVR` | record | supported |  |
| `PUTRETAIN` | record, field | supported |  |
| `RANGE` | field | supported | two literals; input-capable fields |
| `REF` | file | supported | the file REFFLD and R in position 29 look in |
| `REFFLD` | field | supported | field, optional file or *SRC; through the RefResolver |
| `RETCMDKEY` | record | accepted and stored |  |
| `RETKEY` | record | accepted and stored |  |
| `RETLCKSTS` | record | accepted and stored |  |
| `RMVWDW` | record | supported | needs WINDOW on the record |
| `ROLLDOWN` | file, record | supported |  |
| `ROLLUP` | file, record | supported |  |
| `RSTDSP` | file, record | supported |  |
| `RTNCSRLOC` | record | supported | the named fields must exist with the shape IBM documents |
| `RTNDTA` | record | supported |  |
| `SETOF` | record, field | supported | response indicator and text |
| `SETOFF` | record, field | supported | as SETOF |
| `SFL` | record | supported | must be followed by its SFLCTL record |
| `SFLCHCCTL` | field | accepted and stored |  |
| `SFLCLR` | record | supported |  |
| `SFLCSRPRG` | field | accepted and stored |  |
| `SFLCSRRRN` | record | supported | record level, as IBM has it; the field is a hidden zoned 5,0 |
| `SFLCTL` | record | supported | must name the SFL record immediately before it |
| `SFLDLT` | record | supported |  |
| `SFLDROP` | record | supported | CAnn or CFnn |
| `SFLDSP` | record | supported |  |
| `SFLDSPCTL` | record | supported |  |
| `SFLEND` | record | supported | *PLUS *MORE *SCRBAR *TEXT |
| `SFLENTER` | record | accepted and stored |  |
| `SFLFOLD` | record | supported | CAnn or CFnn |
| `SFLINZ` | record | supported |  |
| `SFLLIN` | record | supported |  |
| `SFLMLTCHC` | record | accepted and stored |  |
| `SFLMODE` | record | supported | the field is a hidden character 1 |
| `SFLMSG` | record | supported |  |
| `SFLMSGID` | record | supported |  |
| `SFLMSGKEY` | field | supported | shapes the field: character 4 |
| `SFLMSGQ` | field | accepted and stored | not an IBM keyword; kept for sources that carry it |
| `SFLMSGRCD` | record | supported | makes the subfile a message subfile; SFLMSGKEY and SFLPGMQ fields required |
| `SFLNXTCHG` | record | supported |  |
| `SFLPAG` | record | supported | per display size with *DSn; at most SFLSIZ; the page must fit the display |
| `SFLPGMQ` | field | supported | shapes the field: character 10 or 276 |
| `SFLRCDNBR` | field | supported | CURSOR, *TOP; the field is zoned 4,0 with usage O or H |
| `SFLRNA` | record | supported |  |
| `SFLROLVAL` | field | supported | the field is zoned 4,0 input-capable |
| `SFLRTNSEL` | record | accepted and stored |  |
| `SFLSCROLL` | record | supported | the field is a hidden zoned 4,0 |
| `SFLSIZ` | record | supported | per display size with *DSn; 1 to 9999 |
| `SFLSNGCHC` | record | accepted and stored |  |
| `SLNO` | record | supported | n or *VAR |
| `SNGCHCFLD` | field | accepted and stored |  |
| `SYSNAME` | field | supported | 8 positions |
| `TEXT` | record, field | supported | up to 50 characters |
| `TIME` | field | supported | *SYS/*JOB; 8 positions |
| `TIMFMT` | field | supported | *HMS *ISO *USA *EUR *JIS |
| `TIMSEP` | field | supported | one character or *JOB |
| `UNLOCK` | record | supported |  |
| `USER` | field | supported | 10 positions |
| `USRDFN` | file, record | accepted and stored |  |
| `USRDSPMGT` | file | supported |  |
| `USRRSTDSP` | record | supported | needs WINDOW on the record |
| `VALUES` | field | supported | literals; input-capable fields |
| `VLDCMDKEY` | file, record | supported | the response indicator is required |
| `WDWBORDER` | file, record | accepted and stored | at record level needs WINDOW |
| `WDWTITLE` | record | accepted and stored | needs WINDOW on the record |
| `WINDOW` | record | supported | record, *DFT lines cols, or row col lines cols with &fields; must fit with border and attributes |
| `WRDWRAP` | field | accepted and stored |  |

The subfile keywords belong on one record: `SFL`, `SFLNXTCHG`,
`SFLMSGRCD` and the fields' `SFLMSGKEY`, `SFLCSRPRG`, `SFLCHCCTL`,
`SFLMSGQ` on the subfile record; the rest of the `SFL*` set on the
control record (`SFLPGMQ` on either). A subfile record cannot carry
keys, `WINDOW`, `OVERLAY`, `CLRL`, `SLNO` or the other record-wide
keywords (`DDS6415`).

## 4. Rules

Diagnostics are `DDS6nnn` in the scheme of the `dds` crate: 61xx
columns and conditioning, 62xx keywords, 63xx file level, 64xx records,
65xx fields; keyword syntax, duplicates, conflicts, parameter counts and
unknown keywords are the shared `DDS02xx`. Severities: 00 information,
10 warning, 20 error (no object is created), 30 severe (no record
format at all).

Positions. Each display size of `DSPSIZ` is checked; a field without an
alternate location for the secondary size keeps its primary one. A
displayed field has a row and column (`DDS6105`); hidden and
program-to-system fields, and the `SFLPGMQ`/`SFLMSGKEY` fields, have
none (`DDS6106`). A `+n` column follows the previous field in source
order on its row, wrapping with the display (`DDS6107`). The position
before a field holds its attribute byte, so row 1 column 1 is refused
and two fields of a record may not share a position or its attribute
(`DDS6508`, `DDS6509`). A field may wrap onto the following lines but
not past the display; an input-capable field cannot end in the last
position, its ending attribute needs one more. Inside a window or a
pull-down the area is the usable size and row 1 column 1 is allowed.
Fields are checked regardless of their option indicators, as IBM does.

Screen lengths:

| Field | Positions |
|---|---|
| `'text'`, `DFT('text')` | the characters |
| `DATE` | 6, 8 with `*YY`; 2 more with `EDTCDE(Y)` |
| `TIME` | 8 |
| `USER` | 10 |
| `SYSNAME` | 8 |
| `MSGCON(len ...)` | `len` |
| character | the length; graphic twice the length |
| `L`, `T`, `Z` | the `DATFMT` length (10, 8 or 6), 8, 26 |
| numeric with `EDTCDE` | `qdecimal::code_width` of the shape and code |
| numeric with `EDTWRD` | the characters of the word |
| numeric, output only | the digits |
| numeric, input-capable | the digits, one more for the sign, one more when there are decimal positions |

Subfiles. An `SFL` record must be followed by the record with
`SFLCTL(name)` (`DDS6401`, `DDS6402`). The control record needs `SFLSIZ`
and `SFLPAG` (`DDS6403`), per display size when conditioned by `*DSn`;
the page may not exceed the size (`DDS6404`). The page starts on the
subfile record's first line and takes `SFLPAG` times the record's lines;
it must fit the display (`DDS6405`) and no control-record field may lie
on its lines (`DDS6416`), except with `SFLLIN`. `SFLMSGRCD` makes a
message subfile, whose record needs `SFLMSGKEY` and `SFLPGMQ` fields
and whose control record needs an `SFLPGMQ` field (`DDS6209`). The
fields `SFLCSRRRN`, `SFLMODE`, `SFLSCROLL`, `RTNCSRLOC` and `CSRLOC`
name must exist with the shape IBM documents (`DDS6411`, `DDS6412`).

Windows. `WINDOW(row col lines cols)` places the upper-left corner of
the border; the usable area has `lines` by `cols`. With the border and
its attribute bytes the window takes `lines + 2` rows and `cols + 4`
columns, so `row + lines + 1` and `col + cols + 3` must be within the
display on every size (`DDS6408`). `WINDOW(*DFT lines cols)` leaves the
place to the system and `WINDOW(record)` borrows another record's
window, which must exist (`DDS6409`). `RMVWDW`, `USRRSTDSP`, `WDWTITLE`
and record-level `WDWBORDER` need `WINDOW` on the record.

Menu bars. A `MNUBAR` record needs a field with `MNUBARCHC`, each
naming a `PULLDOWN` record; `MNUBARDSP` names a `MNUBAR` record
(`DDS6417`, `DDS6409`). Pull-down and choice keywords beyond that are
stored.

Keys. `CAnn`/`CFnn`, `ROLLUP`, `ROLLDOWN`, `PAGEUP`, `PAGEDOWN`,
`CLEAR`, `HOME`, `HELP`, `PRINT` and `VLDCMDKEY` take `[indicator
['text']]`; a key given twice at one level, or `CAnn` and `CFnn` with the
same number, is refused (`DDS6208`). File-level keys apply to every
record.

## 5. Buffers

Each record format has an output buffer (what the program writes) and
an input buffer (what it reads):

- The output buffer holds the fields with usage `O` (or blank), `B`,
  `H`, `M` and `P` in source order; the input buffer those with `I`, `B`
  and `H`. Constants have no bytes.
- Without `INDARA`, each buffer begins with one byte per indicator the
  format uses, in ascending order: the option indicators (every
  indicator conditioning a field or keyword of the record) in the
  output buffer, the response indicators (keys at file and record
  level, `SETOF`, `CHANGE`, `DUP`, `BLANKS`, `ERRMSG`, `MNUCNL`...) in
  the input buffer. With `INDARA` both lists are empty and the
  indicators live in a separate 99-byte area, one byte per indicator
  01-99.
- Bytes follow the external type as `docs/DB_FORMAT.md` lays records
  out: character and zoned one byte per position or digit, graphic
  two, packed `digits / 2 + 1`, binary 2, 4 or 8 bytes for up to 4, 9
  or 18 digits, float 4 bytes up to 9 digits and 8 beyond, date its
  `DATFMT` length, time 8, timestamp 26.

The format level identifier is the database's `fmt_level_id`, over the
named fields in source order. `layout::as_field_def` is the adapter: a
display field becomes a `dds::FieldDef` of the database type its buffer
class maps to (`N`, `X`, `Y` and the other display letters become `A`
or `S`), with its length and decimals; constants have no entry. Usage,
positions and attributes do not change the identifier; `INDARA` does
not either.

## 6. Listing

The listing has the `dds` header (compiler, file, source, type), the
source with sequence numbers, the messages under their lines and a
`*` line under a referenced field showing the resolved shape; the
display sizes and file keywords; per record format its kind, subfile
and window summary, keys and record keywords, then a field table
(field, type, length, decimals, usage, row, column, screen length,
output and input offsets, indicators, keywords with `<cond>` after a
conditioned one and `[*DS4:row,col]` alternate locations) and the
buffer line with both lengths, both indicator lists and the level
identifier; the indicator usage (where each indicator conditions and
where it responds, with `INDTXT`); and the messages in source order
with totals per severity. Lines are cut at 132 columns.

## Payload

The display attributes of a `*FILE` object: the fixed header below,
then the model section (`ModelLen` bytes) and the DDS source as
`SourceLines` records of `SourceRecord`. Text in the header is CP037,
blank padded; integers are little-endian.

### `DisplayFile` (64 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSUIDSPF` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 2 | Flags | bit 0 `INDARA`, 1 `RSTDSP`, 2 `USRDSPMGT`, 3 `CSRINPONLY`, 4 `DSPRL`, 5 `ERRSFL` |
| 12 | 1 | Sizes | 1 or 2 display sizes |
| 13 | 1 | Rows1 | lines of the primary size |
| 14 | 1 | Cols1 | columns of the primary size |
| 15 | 1 | Rows2 | 0 without a secondary size |
| 16 | 1 | Cols2 | |
| 17 | 1 | MsgLoc | `MSGLOC`; 0 for none |
| 18 | 2 | Ccsid | file-level `CCSID`; 0 for none |
| 20 | 2 | Records | record formats |
| 22 | 4 | SourceLines | source records after the model |
| 26 | 4 | ModelLen | bytes of the model section |
| 30 | 4 | SourceLen | bytes of the source section, `SourceLines` times 88 |
| 34 | 10 | RefFile | `REF` file, blank for none |
| 44 | 10 | RefLib | `REF` library |
| 54 | 10 | Reserved | zero |

### `SourceRecord` (88 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | Seq | sequence number times 100 |
| 4 | 4 | Date | `yymmdd`, 0 when unknown |
| 8 | 80 | Text | the record, CP037 |

The model section is the `DisplayFile` model written field by field:
little-endian integers, strings as a four-byte length and UTF-8, counts
before lists, one tag byte per enumeration, and an `Option` as a
presence byte. Conditions are the size index (presence byte and value)
and the groups of indicators (number and an on/off byte); keyword
parameters are tagged word, quoted, hex or group; the buffers are their
indicator bytes, their entries (field index, offset, length) and their
length. A damaged section decodes to `PayloadError::Damaged`, a wrong
magic to `NotDisplayFile`, another version to `Version`.

## Open questions

- The sign and decimal-point positions of input-capable numeric fields
  follow the rule above; the runtime will confirm against a real display
  and the rule may move to the data stream encoder.
- Indicator bytes are in ascending order at the front of each buffer;
  IBM's compiler is not documented on the order, and the externally
  described programs of Phase 9 take whatever order this crate fixes.
- Fields conditioned by exclusive indicators may not overlap, as IBM
  refuses; a later phase may relax this for records that need it.
- Window geometry (`lines + 2` by `cols + 4`) comes from the border and
  its attribute bytes; the 5250 writer of Phase 8 will confirm the
  figures.
