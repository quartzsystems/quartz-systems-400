# System APIs

The `sysapi` crate implements IBM-named system APIs with IBM's
parameter lists and receiver formats, as native programs in `QSYS` and
as bound procedures of the service program `QLEAWI`. Generated code
reaches a program API through `sysapi NAME(args)` (a program call into
`QSYS` with every argument by reference) and a `QLEAWI` procedure
through `callb` to an `extern proc` the binder records as `*SYSTEM`
(`ILE.md` §3). Every string an API returns is Quartz-written; the
names, parameter orders, offsets and message identifiers are the
compatibility interface.

Where a format is given here as *QS/400 reading*, IBM's documented
layout was not available to check against and the offsets are this
implementation's; the oracle kit under `tools/conformance/oracle/ile/`
settles them (`ILE_OPEN_QUESTIONS.md`).

## 1. Conventions

- Character fields are EBCDIC (CCSID 37), blank padded. Binary fields
  are big-endian: `bin4` is an `i32`, `bin2` an `i16`. Packed lengths
  (`packed(5,0)`, `packed(15,5)`) are what CL passes.
- A receiver format starts with *bytes returned* (`bin4` at 0) and
  *bytes available* (`bin4` at 4); the API fills as much as the
  receiver length allows and reports the whole size.
- A qualified name is 20 bytes: name then library. A blank library is
  `*LIBL`. `*CURLIB` creates in `QGPL` until the job has a current
  library.
- A format name the API does not know is `CPF3C21`; a type or option
  it does not know is `CPF24B3`; a call-stack counter that reaches
  past the stack is `CPF24A3`; an object that does not exist is
  `CPF9801` (name, library), a library `CPF9810`, a file `CPF9812`, a
  message queue `CPF2403`, a job `CPF3C53`.
- **Error code.** The last parameter of most APIs. `ERRC0100`: bytes
  provided (`bin4` at 0), bytes available (4), exception identifier
  (`char(7)` at 8), reserved (15), exception data (16). `ERRC0200`:
  key `-1` (`bin4` at 0), bytes provided (4), bytes available (8),
  identifier (12), reserved (19), CCSID of the data (20), offset to
  the data (24), length of the data (28), data (32). Bytes provided 0,
  or the parameter omitted, means the error is an `*ESCAPE` message
  to the caller instead. The APIs that have no error code parameter
  (`QCMDEXC`, `QCMDCHK`, `QSNDDTAQ`, `QXXCHGDA`, `QXXRTVDA`) always
  escape.
- **Feedback code** (`QLEAWI`). A 12-byte condition token (`ILE.md`
  §5): all zeros (`CEE0000`) is success; otherwise the token of a
  `CEE` message. Omitted, an error is signaled to the caller.
- **Call-stack entry parameters** (`QMHSNDPM` and friends): `*` is
  the caller of the API, `*CTLBDY` the nearest control boundary,
  `*PGMBDY` the first entry of the caller's program, `*EXT` the
  external queue, any other value the nearest entry whose program or
  procedure has that name; the counter moves that many entries
  towards the caller of the stack.
- **Generic list header** (`QUSGEN`), written at the start of the
  user space by every list API, 192 bytes:

| offset | field |
|---|---|
| 0 | user area, 64 |
| 64 | size of the generic header, `bin4` (192) |
| 68 | structure release and level, `0100` |
| 72 | format name, 8 |
| 80 | API used, 10 |
| 90 | date and time created, `CYYMMDDHHMMSS` |
| 103 | information status: `C` complete |
| 104 | size of the user space used, `bin4` |
| 108 | offset to the input parameter section, `bin4` |
| 112 | size of the input parameter section, `bin4` |
| 116 | offset to the header section, `bin4` |
| 120 | size of the header section, `bin4` |
| 124 | offset to the list data section, `bin4` |
| 128 | size of the list data section, `bin4` |
| 132 | number of list entries, `bin4` |
| 136 | size of each entry, `bin4` |
| 140 | CCSID, `bin4` (37) |
| 144 | country `US`, 146 language `ENU`, 149 subsetted list `0`, 150 reserved |

The input parameter section holds the API's parameters as passed;
the header section is per API and listed with it.

## 2. Messages (`QMH`)

| API | parameters |
|---|---|
| `QMHSNDPM` | message id `char(7)`, message file `char(20)`, data `char(*)`, data length `bin4`, type `char(10)`, call-stack entry `char(10)`, counter `bin4`, message key out `char(4)`, error code |
| `QMHRCVPM` | receiver, length, format `RCVM0100`, entry, counter, type `char(10)`, key `char(4)`, wait `bin4`, action `char(10)`, error code |
| `QMHRMVPM` | entry, counter, key, which `char(10)`, error code |
| `QMHRTVM` | receiver, length, format `RTVM0100`, message id, message file, data, data length, replace `char(10)`, return format control `char(10)`, error code |
| `QMHSNDM` | message id, message file, data, length, type, queues `char(20)` each, count `bin4`, reply queue `char(20)`, key out, error code |
| `QMHRCVM` | receiver, length, format `RCVM0100`, queue `char(20)`, type, key, wait, action, error code |
| `QMHRMVM` | queue, key, which, error code |
| `QMHSNDBM` | text, length, queue, error code |
| `QMHSNDRM` | key, queue, reply, reply length, remove `char(10)`, error code |
| `QMHRSNEM` | key, error code, [entry, counter] |
| `QMHMOVPM` | key, types `char(10)` each, count, entry, counter, error code |
| `QMHCHGEM` | entry, counter, key, action `char(10)`, data, length, error code |

Types are `*INFO`, `*COMP`, `*DIAG`, `*ESCAPE`, `*STATUS`, `*NOTIFY`,
`*RQS`, `*INQ`, `*RPY`. A blank message identifier sends an immediate
message whose text is the data. Receive types add `*ANY`, `*NEXT`,
`*PRV` (relative to the key) and `*BYKEY`; actions are `*SAME`, `*OLD`
and `*REMOVE`. Which: `*ALL`, `*KEEPUNANS`, `*NEW`, `*OLD`, `*BYKEY`.
The non-program queues are `QSYSOPR` and the job's external queue,
named `*EXT`. A message key is the `u32` key big-endian.

`RCVM0100`: severity `bin4` at 8, message id at 12, type code
`char(2)` at 19 (`01` completion, `02` diagnostic, `04`
informational, `05` inquiry, `08` request, `14` notify, `15` escape,
`21` reply), key at 21, reserved 25, CCSID status `bin4` at 32, CCSID
at 36, data length returned and available at 40 and 44, text length
returned and available at 48 and 52, then the data and the text from
56. An empty receive returns 56 bytes with blanks.

`RTVM0100`: message length returned and available at 8 and 12, help
length at 16 and 20, the message from 24 and the help after it.

`QMHCHGEM *HANDLE` marks the pending exception handled when the key
is its message; `*CANCEL` and `*TRANSFER` are accepted.

## 3. User spaces and user indexes (`QUS`)

| API | parameters |
|---|---|
| `QUSCRTUS` | name `char(20)`, attribute `char(10)`, size `bin4`, initial value `char(1)`, authority `char(10)`, text `char(50)`, replace `char(10)`, error code, [domain, transfer size, alignment] |
| `QUSPTRUS` | name, pointer out (16 bytes), error code |
| `QUSRTVUS` | name, start `bin4` (1-based), length `bin4`, receiver, error code |
| `QUSCHGUS` | name, start, length, data, force `char(1)`, error code |
| `QUSDLTUS` | name, error code |
| `QUSCUSAT` | returned library out `char(10)`, name, attributes (count `bin4`, then key `bin4`, length `bin4`, data), error code |
| `QUSRUSAT` | receiver, length, format `SPCA0100`, name, error code |
| `QUSCRTUI` | name, attribute, entry length type `char(1)` (`F`/`V`), entry length `bin4`, key insertion `char(1)`, key length `bin4`, immediate update `char(1)`, optimization `char(1)`, authority, text, replace, error code |
| `QUSDLTUI` | name, error code |
| `QUSADDUI` | name, returned count out `bin4`, entry lengths `bin4` each (variable entries), insert type `char(1)` (`1` unique, `2` replace), entries, entry length `bin4`, count `bin4`, error code |
| `QUSRTVUI` | receiver, receiver length, entry lengths out, entry lengths length, returned count out, name, rule `bin4`, key, key length, error code |
| `QUSRMVUI` | as `QUSRTVUI`, the entries removed |
| `QUSRUIAT` | receiver, length, format `IDXA0100`, name, error code |

A change past the end of a user space grows it (pointers handed out
before die, `MCH3402` on use). `QUSCUSAT` key 1 sets the size; 2 and
3 are accepted. `SPCA0100`: size `bin4` at 8, auto-extend `char(1)` at
12, initial value at 13, library at 14. Index rules: 1 equal, 2
greater, 3 less, 4 not less, 5 not greater, 6 first, 7 last, 8 between
two keys. `IDXA0100` (*QS/400 reading*): reserved `bin4` at 8, entry
length type at 12, entry length `bin4` at 13, keyed at 17, key length
`bin4` at 18, immediate update at 22, optimization at 23, entries
`bin4` at 24, reserved at 28, library at 32, text at 42.

## 4. Objects

| API | parameters |
|---|---|
| `QUSLOBJ` | user space, format `OBJL0100`/`OBJL0200`, object `char(20)` (name, generic `ABC*` or `*ALL`), type `char(10)` (`*ALL` allowed), error code |
| `QUSROBJD` | receiver, length, format `OBJD0100`..`OBJD0400`, object, type, error code |
| `QLICOBJD` | returned library out, object, type, changes (count, then key/length/data), error code |
| `QLIRNMO` | object, type, new object `char(20)`, replace `char(1)`, returned library out, error code |

`OBJL0100` entries: name, library, type (30 bytes). `OBJL0200` adds
information status `char(1)` at 30, extended attribute at 31, text at
41, user-defined attribute at 91, reserved to 108. The header section
holds the name, library and type asked for.

`OBJD0100`: name 8, library 18, type 28, return library 38, ASP `bin4`
48, owner 52, domain 62, creation 64 and change 77 as
`CYYMMDDHHMMSS` (90 bytes). `OBJD0200` adds extended attribute 90,
text 100, source file 150, library 160, member 170 (180 bytes).
`OBJD0300` continues with the save and creation fields to 460, the
system level at 390 being the release; `OBJD0400` returns the same.
`QLICOBJD` keys: 1 text `char(50)`, 2 user-defined attribute
`char(10)` (*QS/400 reading*).

## 5. Data queues and data areas

| API | parameters |
|---|---|
| `QSNDDTAQ` | name `char(10)`, library `char(10)`, length `packed(5,0)`, data, [key length `packed(3,0)`, key, asynchronous, journal] |
| `QRCVDTAQ` | name, library, length out `packed(5,0)`, data out, wait `packed(5,0)`, [key order `char(2)`, key length `packed(3,0)`, key, sender length `packed(3,0)`, sender out, remove `char(10)`, size `packed(5,0)`, error code] |
| `QCLRDTAQ` | name, library, [key order, key length, key, error code] |
| `QMHQRDQD` | receiver, length, format `RDQD0100`, queue `char(20)`, error code |
| `QMHRDQM` | receiver, length, format `RDQM0100`, queue, selection, selection length, error code |
| `QWCRDTAA` | receiver, length, data area `char(20)`, start `bin4`, length `bin4`, error code |
| `QXXCHGDA` | data area, start, length, value |
| `QXXRTVDA` | receiver, receiver length, data area, start, length |

Key orders: `EQ`, `NE`, `GT`, `GE`, `LT`, `LE`. No entry returns
length 0. `RDQD0100`: entry length `bin4` at 8, key length at 12,
sequence `char(1)` at 16 (`F`, `L`, `K`), sender id at 17, force at 18,
text at 19, type at 69, automatic reclaim at 70, entries `bin4` at 72,
allocated at 76, name at 80, library at 90. `RDQM0100` (*QS/400
reading*): entries `bin4` at 8, offset to the first at 12, then each
entry: offset to the next `bin4`, data length, key length, sender
length, key, data.

`QWCRDTAA` receiver: type `char(10)` at 8 (`*CHAR`, `*DEC`, `*LGL`),
library at 18, length `bin4` at 28, decimal positions at 32, the
value from 36. Start -1 reads everything. `*LDA` is the job's local
data area, 1024 characters.

## 6. Job and system

| API | parameters |
|---|---|
| `QUSRJOBI` | receiver, length, format `JOBI0100`/`JOBI0700`, job `char(26)` (`*` for this one), internal id `char(16)`, error code |
| `QUSLJOB` | user space, format `JOBL0100`, job `char(26)`, status `char(10)`, error code |
| `QWCRSVAL` | receiver, length, count `bin4`, names `char(10)` each, error code |
| `QWCRNETA` | receiver, length, count, names, error code |
| `QWCCVTDT` | input format `char(10)`, input, output format `char(10)`, output, error code |
| `QWCRTVCA` | receiver, length, format `RTVC0100`, error code |
| `QWCRTVTZ` | receiver, length, format `RTMZ0100`, time zone `char(10)`, error code |
| `QSZRTVPR` | receiver, length, format `PRDR0100`, product `char(7)` (`*OPSYS`), error code |
| `QLGCNVCS` | request block, input, output, length `bin4`, error code |
| `QCMDEXC` | command, length `packed(15,5)`, [IGC] |
| `QCMDCHK` | command, length `packed(15,5)` |
| `QCAPCMD` | source, length `bin4`, options `CPOP0100`, options length, format, changed command out, its length, available length out, error code |
| `QTNRCMTI` | receiver, length, format `CMTI0100`, error code |
| `QUSLSPL` | user space, format `SPLF0100`/`SPLF0200`, user, output queue, form type, user data, error code |
| `QWDRSBSD` | receiver, length, format `SBSI0100`, subsystem `char(20)`, error code |
| `QWCRJBST` | receiver, length, format `JBST0100`, job `char(26)`, internal id, error code |

`JOBI0100`: job name 8, user 18, number 28, internal id 34, status 50
(`*ACTIVE`), type 60 (`B`), subtype 61, reserved 62, run priority
`bin4` 64, time slice 68, default wait 72, purge 76 (86 bytes).
`JOBI0700`: the same to 62, then system library count `bin4` 62,
product library count 66, current library existence 70, user library
count 74, and the names `char(11)` each from 78. `JOBL0100` entries:
name, user, number, internal id, status, type, subtype (54 bytes).

`QWCRSVAL` and `QWCRNETA` receivers: count `bin4` at 0, an offset
`bin4` per value, and at each offset the name `char(10)`, type
`char(1)` (`C` character, `B` binary), status `char(1)`, length
`bin4` and the data. A name that is not a system value is `CPF1853`,
not a network attribute `CPF1848`.

`QWCCVTDT` formats: `*CURRENT`, `*SYSVAL`, `*JOB` (the clock),
`*DTS` (8 bytes: microseconds since 1928-08-23 12:03:06.314752 UTC,
shifted 12 bits), `*YYMD` `YYYYMMDDHHMMSSmmm`, `*YMD` `CYYMMDD...`,
`*MDYY`, `*MDY`, `*DMYY`, `*DMY`, `*JUL` `CYYDDD...`, `*LONGJUL`
`YYYYDDD...`; a bad format or value is `CPF1846`.

`RTVC0100` (*QS/400 reading*): job name 8, user 18, number 28,
current library 34, user profile 44. `RTMZ0100` (*QS/400 reading*):
offset minutes `bin4` 8, DST `char(1)` 12, name 13, abbreviation 23,
text 33. `PRDR0100`: product id `5770SS1` at 12, release level
`VxRyMz` at 19, option `0000` at 25, load id at 29, the remaining
fields fixed. `QLGCNVCS` request block: type `bin4` (1), CCSID
`bin4`, direction `bin4` (0 upper, 1 lower), reserved 10.

`CMTI0100`
(*QS/400 reading*): commitment definition 8, lock level 18, pending
changes `char(1)` 28, journal 29 and library 39; no commitment
definition is `CPF83E4`. `SBSI0100`: name 8, library 18, status 28
(`*INACTIVE`), jobs `bin4` 38, storage pools 42. `JBST0100`: status
`char(10)` at 8. `QUSLSPL` lists nothing; `QCMDEXC`, `QCMDCHK` and
`QCAPCMD` hand the command to the job's analyzer hook, else to the
provider (`CPF0006` when it has none).

## 7. Database descriptions

| API | parameters |
|---|---|
| `QUSLMBR` | user space, format `MBRL0100`/`MBRL0200`, file `char(20)`, member `char(10)` (`*ALL`, `*FIRST`, generic), override `char(1)`, error code |
| `QUSRMBRD` | receiver, length, format `MBRD0100`/`MBRD0200`, file, member, override, error code |
| `QUSLFLD` | user space, format `FLDL0100`, file, record format `char(10)` (`*FIRST`), override, error code |
| `QUSLRCD` | user space, format `RCDL0100`/`RCDL0200`, file, override, error code |
| `QDBRTVFD` | receiver, length, returned file out `char(20)`, format `FILD0100`, file, record format, override, system `char(10)`, format type `char(10)`, error code |
| `QDBLDBR` | user space, format `DBRL0100`, file, record format, member, error code |

`MBRL0200` entries: name, source type 10, creation 20 and change 33
as `CYYMMDDHHMMSS`, text 46, text CCSID `bin4` 96 (100 bytes).
`MBRD0100`: file 8, library 18, member 28, file attribute 38, source
type 48, creation 58, change 71, text 84, source file 134, remote
135, logical 136, ODP sharing 137, reserved 138 (140 bytes);
`MBRD0200` adds current records `bin4` 140, deleted 144, data size
148, access path size 152, based-on members 156, change date 160.
`FLDL0100` entries (266 bytes): name, data type `char(1)` 10, use 11,
output 12 and input 16 buffer positions `bin4`, length 20, digits 24,
decimal positions 28, text 32, edit code 82, edit word length 84 and
word 88, column headings 152, 172, 192, internal name 212, alternative
name 222 and its length 252, date format 256, separator 260,
variable-length 261, CCSID 262. `RCDL0200` entries: name, record
length `bin4` 10, field count 14, text 18 (68 bytes). `FILD0100`
(*QS/400 subset*): file type `char(1)` at 8 (`P`/`L`), keyed 9,
source 10, record length `bin4` 12, format count 16, member count 20,
based-on count 24 and the names `char(20)` each. `DBRL0100` entries:
dependent file, library, member, kind `L` or `C` (32 bytes).

## 8. `QLEAWI`

The condition APIs (`CEEHDLR`, `CEEHDLU`, `CEERTX`, `CEEUTX`,
`CEESGL`, `CEENCOD`, `CEEDCOD`, `CEEMRCR`) are the runtime's own
(`mi::cee`, `ILE.md` §5). The rest:

| procedure | parameters |
|---|---|
| `CEE4ABN` | none: cancel to the control boundary with `CEE9902` |
| `CEETREC` | [return code `bin4`], [user return code `bin4`]: every entry ends with `CEE9903` pending, which no handler sees |
| `CEEDOD` | position, type, datatype, info1, info2, length, fc: the caller's descriptor of that argument (`ABI.md` §3); `CEE0820` when the caller passed none or the position is past them |
| `CEEGPID` | version out (1), platform out (3), fc |
| `CEETSTA` | presence out `bin4`, argument number `bin4`, fc |
| `CEECRHP` | heap id out, initial size, increment, strategy, fc |
| `CEEDSHP` | heap id, fc (`CEE0803` unknown heap) |
| `CEEGTST` | heap id, size, address out (16-byte pointer), fc |
| `CEEFRST` | address, fc (`CEE0802` not a heap block) |
| `CEECZST` | address in/out, new size, fc |
| `CEEDAYS` | date `VSTRING`, picture `VSTRING`, Lilian day out `bin4`, fc |
| `CEEDATE` | Lilian day, picture, output `char(80)`, fc |
| `CEESECS` | timestamp `VSTRING`, picture, Lilian seconds out `float8`, fc |
| `CEEDATM` | Lilian seconds, picture, output `char(80)`, fc |
| `CEEUTC` | Lilian day out, seconds out (UTC) |
| `CEEGMT` | Lilian day out, seconds out, fc |
| `CEELOCT` | Lilian day out, seconds out, Gregorian `char(23)` out (`YYYYMMDDHHMISS999`), fc |
| `CEEGMTO` | hours out, minutes out, seconds out `float8`: the local offset |
| `CEEQCEN` | century window out `bin4` (80), fc |
| `CEESCEN` | century window 0..100, fc |
| `CEERAN0` | seed in/out `bin4` (0 seeds from the clock), random out `float8` in (0, 1): a Lehmer generator, 16807 modulo 2^31 - 1 |
| `CEE4RAGE` | procedure pointer, fc: an exit called when the activation group ends, with mark, reason (1), result and user return code by reference |

A `VSTRING` is a two-byte big-endian length and the characters. A
`float8` is IEEE double, big-endian in storage. Pictures use `YYYY`,
`YY`, `MM`, `Mmm`, `DD`, `DDD`, `HH`, `MI`, `SS`, `999` and literal
characters; a bad picture is `CEE2518`, a value that does not fit it
`CEE2501`. Lilian day 1 is 1582-10-15; Lilian seconds count from
1582-10-14 00:00:00. The century window decides a two-digit year.

## 9. The provider

The APIs' objects come from a `Provider`: object lists and
descriptions, user indexes, data queues, data areas, system values and
network attributes, the clock, commands, file descriptions, commitment
control and user profiles. The service console implements it over the
store; `MemProvider` keeps everything in memory for tests and the IR
driver, with `*LDA` and the three network attributes preset and system
values from the catalog's defaults. `install` registers the programs
as natives of `QSYS` and the procedures as `*SYSTEM` imports for the
loader, and keeps the provider for the procedures that need the
clock.

## 10. Security

The security APIs live in `sysapi/src/security.rs`. Each asks the
provider one `sec::api::Query`; in a job the storage service answers
it as the job's user (`stg` request 129), so the authority an API
needs is checked where the objects are.

| API | parameters |
|---|---|
| `QSYRUSRI` | receiver, length, format `USRI0100`/`0200`/`0300`, user `char(10)` (`*CURRENT`), error code |
| `QSYCUSRS` | indicator out `char(1)`, user, special authorities `char(10)` each, count `bin4`, call level, error code |
| `QSYCUSRA` | indicator out `char(1)`, user, object `char(20)`, type, authorities `char(10)` each, count, call level, error code |
| `QSYRUSRA` | receiver, length, format `USRA0100`, user, object `char(20)`, type, error code |
| `QSYLOBJA` | user space, format `OBJA0100`/`0200`, user, type, `*ALL`/`*OBJOWN`/`*OBJAUT`, continuation handle `char(20)`, error code |
| `QSYLOBJP` | user space, format `OBJP0100`, user, type, error code |
| `QSYRAUTU` | receiver, length, format `RTUA0100`, feedback `char(16)`, object `char(20)`, type, error code (a QS/400 reading) |
| `QSYRTVUA` | receiver, length, feedback `char(16)`, format `RTUA0100`, object `char(20)`, type, error code (IBM's order) |
| `QSYLAUTU` | user space, format `AUTU0100`, `*ALL`/`*USRPRF`/`*GROUP`/`*MEMBER`, group, error code |
| `QSYLATLO` | user space, format `ATLO0100`, list `char(10)`, error code |
| `QSYGETPH` | user, password `char(10)` (or `*NOPWD`, `*NOPWDCHK`, `*NOPWDSTS`), handle out `char(12)`, error code [, password length, CCSID] |
| `QWTSETP` | handle, error code |
| `QSYRLSPH` | handle, error code |
| `QSYCHGPW` | user, current password, new password, error code [, lengths and CCSIDs] |
| `QSYRUPWD` | receiver, length, format `UPWD0100`, user, error code |
| `QSYSUPWD` | data, format `UPWD0100`, error code |

`USRI0100`: user 8, previous sign-on 18, reserved 31, invalid sign-on
attempts `bin4` 32, status 36, password change date 46, no password
54, reserved 55, expiration interval `bin4` 56, reserved 60, days to
expiration `bin4` 68 (-1), set to expire 72, display sign-on
information 73, local password management 83, block password change
84 (94 bytes). `USRI0200`: user 8, then from 18 class, special
authorities `char(15)` (`Y`/`N` in `*ALLOBJ *SECADM *JOBCTL *SPLCTL
*SAVSYS *SERVICE *AUDIT *IOSYSCFG` order, seven reserved), group,
owner, group authority, limit capabilities, group authority type,
reserved 3, supplemental groups offset and count `bin4`, text 50, the
groups `char(10)` each. `USRI0300` is laid out as an IBM i 7.1 returns
it (`tools/conformance/oracle/sec/expected/usri0300.txt`): `USRI0100`
to 72, then class 73, special authorities `char(15)` 83, group 98,
owner 108, group authority 118, assistance level 128, current library
138, initial menu 148 and library 158, initial program 168 and library
178, limit capabilities 188, text 198, display sign-on information 248,
limit device sessions 258, keyboard buffering 268, reserved 278,
maximum storage `bin4` 280, storage used 284, priority limit `char(1)`
288, job description 289 and library 299, accounting code `char(15)`
309, message queue 324 and library 334, delivery 344, reserved 354,
severity `bin4` 356, output queue 360 and library 370, printer 380,
special environment 390, attention program 400 and library 410,
language 420, country 430, CCSID `bin4` 440 (-2 for `*SYSVAL`), user
options 444 (seven `Y`/`N`, blank to 480), sort sequence 480 and
library 490, object auditing 500, action auditing 510 (31 `Y`/`N`,
blank to 574), group authority type 574, supplemental groups offset
`bin4` 584 and count 588, UID 592, GID 596, home directory offset 600
and length 604, locale job attributes 608 (eight `Y`/`N`), reserved
616, locale offset `bin4` 624 and length 628, two bytes `00` at 632
and a `char(10)` `*SYSVAL` at 634 (the capture shows them; read here
as a reserved pair and `CHRIDCTL`), zeros 644, local password
management 660, block password change 661, a byte `1` at 671 (not
identified), `bin4` 0 at 672, user expiration date 676, user
expiration interval 684; then the home directory as a
`Qlg_Path_Name_T` (CCSID 1200, `US`, `ENU`, path type, path length,
`/` delimiter, the path in UCS-2) at 694, the locale after it, and the
supplemental groups last.

An authority in a receiver is its name (`*ALL`, `*CHANGE`, `*USE`,
`*EXCLUDE`, `USER DEF`; `*AUTLMGT` for a list) and eleven `Y`/`N`
flags: `*OBJOPR *OBJMGT *OBJEXIST *OBJALTER *OBJREF`, a reserved
byte, `*READ *ADD *UPD *DLT *EXECUTE`. `USRA0100`: authority 8,
`*AUTLMGT` 18, flags 19, source 30 (the search step), list 46,
owner 56, primary group 66. `OBJA0100`/`OBJP0100` entries (62 bytes):
object, library, type, owner, owned `Y`/`N`, authority, flags.
`RTUA0100` entries (40 bytes): profile, group `Y`/`N`, authority,
`*AUTLMGT`, flags, reserved; the owner first, `*PUBLIC` last; the
feedback holds bytes returned and available, the count and the entry
length. `AUTU0100` entries: profile, group, has members, text.
`ATLO0100` entries: object, library, type, owner.

`QSYGETPH` checks the password as sign-on does (`CPF22E2` wrong,
`CPF22E3` disabled, `CPF22E4` expired, `CPF22E5` none) or, for the
`*NOPWD` values, `*USE` to the profile; `QWTSETP` switches the job's
user in the storage service, keeping its adopted authority, and both
are audited (`PS`). `QSYCHGPW` runs `QPWDVLDPGM` in the job first,
then changes the password as `CHGPWD` would. `QSYRUPWD` and
`QSYSUPWD` need `*ALLOBJ` and `*SECADM`; their data is the magic
`QS400PWD` and the profile's hashes, not IBM's format, so it moves
between QS/400 systems only.
