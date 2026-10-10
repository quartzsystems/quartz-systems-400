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
