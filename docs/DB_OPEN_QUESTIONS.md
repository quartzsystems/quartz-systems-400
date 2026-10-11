# Database: open questions

Semantics the Phase 3 code implements by best guess. Each entry names
the behaviour QS/400 has today and the test that would settle it on a
real IBM i. When an answer arrives, fix the code, add the case to the
conformance kit and delete the entry.

## DDS compiler

**DATSEP/TIMSEP(*JOB) in a physical file.** Rejected (DDS0206). The DDS
reference describes `*JOB` for display and printer files; a database
field needs a fixed separator because it decides the stored bytes.
Test: `CRTPF` a field `L DATFMT(*MDY) DATSEP(*JOB)` and see whether the
compile fails and, if not, what `DSPFFD` reports as the separator.

**Binary fields of 10 to 18 digits.** Accepted and stored in 8 bytes
(`B 1-4` is 2 bytes, `5-9` is 4). Test: `CRTPF` with `B 18 0`, then
`DSPFFD` for the buffer length, and write `99999999999999999` through
RPG to see what the record holds.

**Floating point longer than 9 digits without FLTPCN.** Error DDS1032:
`*SINGLE` is the default and takes at most 9 digits. Test: `CRTPF` with
`F 15 2` and no `FLTPCN`; does the compile fail or silently choose
`*DOUBLE`?

**TEXT at the file level.** Accepted and kept as the file's text (the
PF has one format, so the record text would otherwise be the only
text). Test: a DDS source with `TEXT('x')` before the `R` line.

**REFFLD(... *SRC) when the field is defined twice earlier.** The
latest earlier definition wins. Test: two earlier fields of the same
name in different formats of an LF, then a `*SRC` reference.

**`+` continuation inside a quoted string.** The blanks before the `+`
are kept and the next line's leading blanks are dropped, so
`TEXT('A +` / `B')` is `A B`. Test: compile it and read the text with
`DSPFFD`.

**Validity keywords on a reference field.** Any of `CHECK`, `VALUES`,
`RANGE`, `COMP` on the referencing field drops all four from the
referenced one; `EDTCDE` or `EDTWRD` drops both. Test: reference a
field that has `RANGE` and add `CHECK(ME)`; `DSPFFD` shows what is kept.

**A dangling continuation sign.** The text so far is parsed anyway, so
`TEXT('x' +` followed by a new entry reports both DDS0103 and the
unbalanced keyword. IBM may report only the first.

## Logical files

**Select/omit on a field outside the format.** Rejected (DDS4009): the
`S`/`O` field must be a field of the logical file's format. Test: an LF
with a field subset and `S CUSTYP COMP(EQ 'A')` where `CUSTYP` is not
in the subset.

**A join format with no field lines.** Takes every field of the primary
file (file 1), as a `PFILE` format without field lines takes every
field of its file. Test: `CRTLF` a join LF with `R`, `J` and `K` lines
only, then `DSPFFD`.

**CONCAT typing.** Zoned when every part is zoned (digits added, no
decimals), else character over the parts' bytes; packed and binary
parts are allowed as bytes, dates, times, timestamps, floats and
graphic fields are not (DDS4016). Test: `CONCAT` of a packed and a
character field, `DSPFFD` for type and length.

**SST of a zoned field.** The result is character. Test: `SST(ZONED 1
2)` and `DSPFFD`.

**Key attributes across formats.** A keyed multi-format LF needs keys
(or `K *NONE`) in every format, but the key fields' types and lengths
are not required to match between formats. Test: two formats keyed on
`P 7` and `A 10`; does `CRTLF` fail?

## Decimal arithmetic

**`*RESDECPOS` semantics.** Implemented as a floor: when the 63-digit
limit forces decimal positions to be dropped from an intermediate
result, they are never dropped below the result field's decimals, and
the integer digits shrink instead (size error if the value then does
not fit). Test: `EVAL(R)` and plain `EVAL` of `A * B / C` with `A`, `B`
40,20 into a 15,5 result, compare values; the qdecimal fuzzer covers
this.

**Nonzero pad nibble in an even-digit packed field.** Treated as a
decimal data error (MCH1202). Test: write `X'1123C'` into a `P 2,0`
field with `CHGPF`-free means (CPYF *NOCHK from a character file) and
read it with RPG.

**Zones before the sign byte of a zoned field.** Must be `F`; blanks
are a decimal data error, as RPG reports without `FIXNBR(*ZONED)`.
Test: a zoned field holding `X'40404040C1'` read by RPG and by SQL.

**Binary field holding more than its digits.** Returned as held
(`B 4,0` with 32767 reads as 32767). RPG truncates to the declared
digits on read; `db` will decide which view record I/O exposes. Test:
write 32767 through SQL into a `B 4,0` column and read it back in RPG.

## Files and members

**Keys longer than 1014 bytes.** The store's B+tree key is at most 1024
bytes and the access path key ends in a 2-byte format tag and the 8-byte
RRN, so a file whose encoded key exceeds 1014 bytes is refused at create;
DDS allows 2000.
Test: `CRTPF` with two `A 1000` key fields on the real machine works;
here it fails. A raise needs the tree to accept keys over a quarter of
the page.

**Message for a full member.** `SIZE` reached is a typed error without a
message ID. Test: fill a `SIZE(5 0 0)` member on the machine and note
the CPF message the write fails with.

**Order of reused slots.** With `REUSEDLT(*YES)` the lowest free RRN is
reused first. Test: delete RRNs 4 and 2, write twice, `DSPPFM` the
RRNs.

**Varlen storage.** A `VARLEN` field is stored at its maximum length in
the slot; the `ALLOCATE` length and overflow are not implemented, so
`DSPFD` size figures differ from the machine's for such files.

## Access paths

**`ALTSEQ`.** Not applied to key bytes; the table name is kept. Test: a
keyed PF with `ALTSEQ` and mixed-case data, read in key order.

**Null key fields under `DESCEND`.** The null byte is complemented with
the rest of the part, so a null sorts low in a descending key. The
machine may still sort nulls high. Test: a `DESCEND` key over an
`ALWNULL` field with null and non-null records.

**Two-digit years in date keys.** `*MDY`, `*DMY`, `*YMD` and `*JUL`
keys read 40 to 99 as 1940 to 1999 and 00 to 39 as 2000 to 2039. Test:
a date key over `DATFMT(*MDY)` with 12/31/39 and 01/01/40, read in key
order.

**`CONCAT` of zoned fields.** Interior parts have their sign zones
forced to `F`; the last part keeps its sign; the result is zoned when
every part is zoned. Test: `CONCAT` of two `S` fields holding -1 and 2.

**Select/omit default.** Records no statement applies to are selected
when the last statement is an omit and omitted when it is a select.
Test: `S A COMP(EQ 'X')` alone, and `O A COMP(EQ 'X')` alone, with
records of both kinds.

**Join logical files.** The path and key come from the primary file;
the record is derived from the primary record with secondary fields
blank. Secondary lookups come with the open data path. Test: none yet.

**Message for a keyed read of an invalid path.** `PathInvalid` carries
CPF5090 as a placeholder. Test: open a `MAINT(*REBLD)` LF for keyed
input after a write and note the message.

## Record-level access

**Lock release on a failed read.** Every read operation first releases
the record held for update, so a `CHAIN` that finds nothing or a `READ`
at end of file leaves no lock. Test: `CHAIN` with lock, then a `CHAIN`
to a missing key, then `UPDATE`: 01221 here.

**Position after a not-found `CHAIN` by key.** The file is positioned
before the first record with a key above the argument, so a `READ` gives
that record. Test: `CHAIN` to a key between two records, then `READ`.

**Position after an `UPDATE` that changes the key.** The next `READ`
continues after the record's old key, so a record whose key moved up may
be read again. Test: update a key field to a higher value, then `READ`.

**`READE` with a mismatch.** `%EOF` comes on and the position does not
move, so a following `READ` returns the record that did not match.
Test: `SETLL` on a prefix, `READE` past its last record, then `READ`.

**`SETLL` with no record.** `%FOUND` is off and the status stays 00000;
`CHAIN` not found is 00012. Test: `SETLL` beyond the last key and check
`*STATUS`.

**Writes through logical files.** `WRITE`, `UPDATE` and `DELETE` on an
open data path over a logical file are refused for now; the machine
allows them when every based-on field is mapped. Test: none yet.

**Messages.** `NoLockedRecord` carries CPF5035 as a placeholder for the
condition RPG reports as 01221 (RNX1221); `FileLocked` carries CPF4128.

**Shared open data path with other usage.** A `SHARE(*YES)` open that
needs input, output, update or delete the existing path was not opened
for fails with `ShareMismatch` (CPF4123 as a placeholder). The machine
may instead open the path with the union of usages when the first open
has not read yet. Test: `OVRDBF SHARE(*YES)`, `OPNDBF OPTION(*INP)`,
then call an RPG program that updates the file.

**Position of a shared path on a later open.** The second program sees
the path where the first left it, and its `POSITION` override is not
applied. Test: two programs sharing a file, the first reads two records,
the second reads one.

**`POSITION(*KEYB)` before the first record.** When no record is below
the key the first read returns the first record of the file. Test:
`OVRDBF POSITION(*KEYB 1 CUSREC 0)` on a file whose lowest key is 1,
then `READ`.

**`POSITION(*RRN n)` on a keyed logical file.** Refused; on a keyed
physical file the reads go on in key order from that record. Test:
`OVRDBF POSITION(*RRN 4)` over a keyed LF, read twice.

**`WAITRCD`.** Kept in the open options; a locked record fails at once
with CPF5027 whatever the wait, since one store transaction runs at a
time (`MIGRATE(phase5)`: wait on the lock table when jobs exist). Test:
none until then.

**`SEQONLY` and `NBRRCDS`.** Kept and ignored: records are never
blocked, so no read ever misses another job's write. Test: none.

## Utilities

**`RGZPFM KEYFILE` with a select/omit logical file.** Records the
logical file leaves out follow the selected ones in arrival sequence.
Test: `RGZPFM KEYFILE(lib/lf mbr)` over an LF with `S` statements; does
the command fail, or where do the omitted records land?

**`CPYF FMTOPT(*MAP)` with extra from-fields.** Refused: `*DROP` must
be given too. `*DROP` alone is refused when a to-field has no source.
Test: `CPYF FMTOPT(*MAP)` between files where each has a field the
other lacks.

**`CPYF MBROPT(*UPDADD)` on a non-unique to-member.** Behaves as `*ADD`;
with a `UNIQUE` path the record with the same key is updated in place.
Test: `*UPDADD` into a keyed file without `UNIQUE`.

**`CPYF` duplicate keys and `ERRLVL`.** A duplicate key, a mapping
error and a wrong record length each count as one recoverable error;
the copy stops when the count exceeds `ERRLVL` and the records written
so far stay (the caller's transaction decides). Test: `CPYF ERRLVL(1)`
with two duplicates and note the completion message counts.

**`CPYF FMTOPT(*CVTSRC)` sequence numbers.** Records are numbered
1.00, 2.00, ... and dated 0. Test: `CPYF` from a data file into a source
file and `DSPPFM` the sequence and date columns.

**`CPYTOIMPF` defaults.** `RCDDLM(*EOR)` writes CR LF; `RMVBLANK(*NONE)`
keeps a character field at its full length; a null field under
`NULLS(*NO)` is written as its stored bytes (blanks, zero, an empty
`VARLEN`); under `NULLS(*YES)` it is empty and unquoted. Dates and
times are written as stored unless `DATFMT`/`TIMFMT` convert them.
Test: `CPYTOIMPF` of a file with a null date and compare the stream file.

**`CPYFRMIMPF` and `VARLEN`.** Trailing blanks of a `VARLEN` field are
dropped, so a `*FIXED` import gives the data length, not the column
width. An empty field is null when the field allows it, else zero or
blanks. Test: `*FIXED` import into a `VARLEN(20)` field, `DSPPFM` in hex.

**`CLRPFM` and `RGZPFM` allocation.** The member is allocated `*EXCL`
and the file `*SHRUPD`; `CPYF` holds the from-member `*SHRRD` and the
to-member `*SHRUPD` (`*EXCL` under `MBROPT(*REPLACE)`). The in-use
error carries CPF4128 like every allocation failure here; the machine
reports CPF3156 for `CLRPFM`. Test: `CLRPFM` while a program has the
member open.

**`CHGPF SRCFILE` with a changed record format.** Refused when the
level identifier, the keys, `UNIQUE` or the duplicate order changed; the
machine converts the data through a temporary copy. Only text, headings,
edit words, defaults and the like change in place here. Test: `CHGPF
SRCFILE` adding a field; the data survives on the machine, here the
command fails.

**`DSPDBR` based-on members.** The relations come from each logical
member's based-on members, so an LF member over a named PF member shows
that member; `DSPFD TYPE(*JOIN)` shows the based-on files with the record
format they supply. Test: `DSPDBR` of an LF over `PF MBR(ARCH)`.

**`DSPPFM` of a logical member.** Shows the derived records in arrival
sequence of the first based-on member, select/omit applied, no deleted
slots. Test: `DSPPFM` of a select/omit LF on the machine.

**Source member dates.** `srcput` dates every record with the current
day as `yymmdd` and numbers from 1.00 in steps of 1.00. Test: `SEU` or
`CPYFRMSTMF` on the machine and `DSPPFM` the sequence column.

## Journaling

**Entry message IDs.** `ReceiverUsed` carries CPF7010, `ReceiverAttached`
CPF7022, `AlreadyJournaled` CPF7030, `NotJournaled` CPF7032,
`ObjectsJournaled` CPF7021, `NoReceiver` CPF7006 and a bad range CPF7054,
all as placeholders. Test: provoke each on the machine and note the
escape message.

**Job names in entries.** Every entry says job `QS400`, user `QSECOFR`
and the job handle as the number until jobs exist (`MIGRATE(phase5)`).
The conformance diff masks these columns.

**`R DL` carries the deleted record.** The delete entry holds the image
of the record as it was, whatever `IMAGES` says, so a rollback can put it
back. Test: `DSPJRN` of a delete under `IMAGES(*AFTER)` and check the
entry-specific data length.

**`F MC` from a copy.** `CPYF MBROPT(*REPLACE)` clears the to-member
through the same path as `CLRPFM`, so it writes `F MC`; `RGZPFM` writes
`F MC`, one `R PT` per surviving record, then `F MR`. The machine writes
`F MR` alone. Test: `RGZPFM` of a journaled member, `DSPJRN`.

**Receiver swap at the threshold.** The threshold is checked before an
entry is written: when the attached receiver already holds at least
`THRESHOLD` KiB, `J NR` closes it, a receiver named by the trailing
digits plus one is created in its library and `J PR` opens it, and the
entry follows. `DLTRCV(*YES)` is kept but nothing deletes receivers yet.
Test: `CHGJRN JRNRCV(*GEN)` name generation on the machine with `RCV9999`.

**Deleted receivers in the directory.** `DLTJRNRCV` leaves the journal's
directory entry; `DSPJRN` skips receivers that are gone. Test: `WRKJRNA`
after deleting the oldest receiver.

## Commitment control

**Cycle identifier.** The sequence number the first `C BC` of the cycle
takes in the first journal touched; a second journal joining the cycle
writes its own `C BC` with the same identifier. Test: a cycle over files
in two journals, `DSPJRN` both.

**Before images under commitment control.** An update under a cycle
journals `R UB` even for `IMAGES(*AFTER)`, so a rollback can undo it;
outside a cycle `*AFTER` files get no before image and `RMVJRNCHG` skips
their updates. Test: `DSPJRN` of an update under commitment control on
an `IMAGES(*AFTER)` file.

**Rollback entries.** Undoing a write deletes the record with `R DR`,
undoing an update writes `R BR` and `R UR`, undoing a delete writes
`R PT` with the rollback flag; then `C RB`. `APYJRNCHG` and `RMVJRNCHG`
flag what they write (bit 3) and skip flagged entries when run again.
Test: `DSPJRN` after `ROLLBCK` and after `RMVJRNCHG` on the machine.

**`ENDCMTCTL` with files open.** Refused with CPF8356 while a path opened
with `COMMIT(*YES)` is open; with nothing open an unfinished cycle is
rolled back first. Test: `ENDCMTCTL` from a program with the file open.

**Lock levels.** `*CHG` locks records read for update and keeps changed
ones to the boundary; `*CS` locks every record read until the next read
and changed ones to the boundary; `*ALL` keeps every record read. The
record a path still holds at `COMMIT` is released too. Message IDs
CPF4293, CPF8350, CPF8351 and CPF8355 are placeholders. Test: `*CS` with
two jobs reading the same record.

**Recovery.** After an abnormal end every journal in every library is
scanned; a cycle with `C BC` and no `C CM` or `C RB` is rolled back from
its record entries and closed with `C RB`. `CMTBDY(*YES)` for
`APYJRNCHG`/`RMVJRNCHG` keeps only entries outside a cycle or in a cycle
that reached `C CM` or `C RB` within the range. Test: pull the power
mid-cycle, IPL, `DSPJRN`.

## SQL

**Reserved words as names.** A word in the parser's reserved list cannot
be an implicit correlation name or a bare column (`SELECT FROM` is a
syntax error); any word is accepted after `AS` or in double quotes. The
machine's list is longer. Test: `SELECT * FROM t order` on the machine.

**`IN` without a parenthesis.** `IN` starts a predicate only when `(`
follows, so `POSITION('a' IN name)` parses as the function's keyword
form. Test: none needed.

**`||` precedence.** `CONCAT` and `||` bind like `*` and `/`, above `+`
and `-`, as the DB2 reference orders them. Test: `SELECT 1 + 2 || '3'`
on the machine (a conversion error there confirms the grouping).

**Unsupported statements.** Common table expressions, `EXCEPT`,
`INTERSECT`, `CALL`, `SET TRANSACTION`, `CREATE SCHEMA`, LOB types,
hidden columns and derived indexes answer SQLCODE -84 (SQLSTATE 42612).
The machine would run them. Test: none.

## Editing

Every item here is covered by the qdecimal oracle kit
(`conformance/oracle/qdecimal`); the machine's `results.csv` settles it.

**Leading zero before the point in edit codes.** Suppressed: `%EDITC` of
0.05 with `J` gives `.05`, a zero balance `.00`.

**The stop position of an edit word.** The digit at the `0` stop prints
even when zero, so `'  0.  '` gives `0.56` for 0.56 and `'   .  '` gives
`   56` (the point is an insertion character in the suppressed region).

**Asterisks in an edit word.** Any `*` among the digit positions makes
asterisks the fill but does not stop suppression; `'**   '` with 12
gives `***12`.

**Floating currency in an edit word.** The `$` left of the stop gives its
position to the digits and lands just left of the first printed digit:
`'  ,  $0.  '` with 12345.67 gives `$12,345.67`.

**Floating minus (codes `N`-`Q`) with `*CURSYM`.** The minus goes left
of the symbol: `-$12.34`.

**`Z` of zero and `*ASTFILL` with a blank-balance code.** Both print
blanks.

**Edit code `Y` groups.** 7 digits is `nnn/nn/nn`; 8 digits is
`nn/nn/nnnn`, or `nnnn/nn/nn` under `DATEDIT(*YMD)`; 9 digits is
`nnn/nn/nnnn`, or `nnnnn/nn/nn` under `*YMD`.

**DATSEP/TIMSEP(*JOB) and REF in logical files.** `REF`, `REFFLD` and
file-level `CCSID` are not meaningful in an LF; `REFFLD` is rejected
(DDS0207), `REF` and `CCSID` are accepted and ignored.
