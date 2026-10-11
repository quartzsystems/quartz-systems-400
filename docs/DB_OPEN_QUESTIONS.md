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
