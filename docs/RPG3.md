# RPG III (RPG/400): `CRTRPGPGM` and `CVTRPGSRC`

Licensed program `QS00RPG` (`*BASE`, library `QRPG`, needs `QS00WDS`
option 31) holds the RPG/400 compiler `CRTRPGPGM` and the source
converter `CVTRPGSRC`. IBM ships RPG/400 as 5770WDS option 32 and
`CVTRPGSRC` with the ILE RPG compiler; QS/400 keeps both in one product
because one crate does both: `rpg3` (`rpg/rpg3`).

## Design

RPG/400 is not a second compiler. `CVTRPGSRC` exists so that every
RPG III program can become an ILE RPG program that does the same
thing, and that is what `CRTRPGPGM` does inside: the member is
converted in memory (`rpg3::cvt`), the conversion is compiled by the
ILE RPG compiler (`rpgc`, `docs/RPG.md`) as `CRTBNDRPG
DFTACTGRP(*YES)` would, and the program runs in the default
activation group as an OPM program does. The cycle, files, the edit
codes and words, the operations, the indicators, the PSDS and INFDS are
the ILE RPG compiler's and runtime's (`qrpgrt`), which already follow
IBM's rules where the two languages agree. What differs is the source
form, which the converter handles, and the reporting:

- **Diagnostics** are reported as RPG/400 reports them: `QRGnnnn` on
  the RPG III statement (its sequence number) the converted line came
  from. RPG/400 and ILE RPG share most numbers (`RNF7030` is
  `QRG7030`); `rpg3::crt::msgs` holds the ones that differ or that
  RPG/400 does not give. The checks RPG/400 makes that ILE RPG does not
  are made on the RPG III source: an overflow indicator nothing uses
  (`QRG7031`), a program-described output file no output specification
  names (`QRG7064`, 40). Names the ILE compiler's cross-reference finds
  unreferenced are `QRG7031` on their defining statement.
- **The listing** has RPG/400's sections (options, source by statement
  number with the diagnostics after their statements, additional
  diagnostics, cross-reference markers, message summary, final
  summary), Quartz texts. The oracle tool reads both listings the same
  way (`rpg_oracle::listing::diags_rpg3`).
- **Messages**: `QRG9001` ends a compile that made nothing (the
  oracle's escape), `QRG0003` a compile that made the program. Message
  file `QRPG/QRPGMSG` (`runtime/msgf/msgs/qrpgmsg.toml`) holds them, the
  diagnostics and the converter's `RNM` messages.

RPG/400 checks the ILE compiler relaxes are not all made: a literal
longer than its factor (`QRG5167`), a search argument of another length
than the array's elements (`QRG7021`) or the key's (`QRG7078`), an edit
word whose replaceable positions do not fit the field (`QRG8004`), a
`TESTB` of one bit with the mixed indicator (`QRG5117`), a numbered
record sequence after an `NS` record (`QRG4101`) and `*LIKE DEFN`'s
length adjustment without its blank (`QRG5081`) compile here. Those are
compile errors on IBM i, not differences in what a program does; the
corpus has none left. The unreferenced-name checks follow RPG/400:
names and indicators set and never used (`QRG7031`, the statement that
first sets them; a field of an externally described file is
statement 0), not files.

## Conversion rules

Each RPG III record is cut with the RPG III column tables
(`rpg/specs/specs/rpg3.toml`, `rpgspecs::rpg3_form`) and written with
the ILE RPG ones (`specs/rpgle.toml`). Columns 1-5 and comments are
kept; the order becomes H, F, D, I, C, O, compile-time data.

| RPG III | ILE RPG |
|---|---|
| `H` columns 15, 18-21, 26, 43 | `DEBUG`, `CURSYM`, `DATEDIT`, `DECEDIT`, `ALTSEQ(*SRC)`, `FTRANS`; a blank `H` stays blank |
| `F` 7-14, 15-19, 24-32, 40-46, 66 | 7-16, 17-22 (the addition `A` to 20), 23-35, 36-42 |
| `F` overflow indicator 33-34 | `OFLIND(*INxx)` |
| `F` key location 35-38 | `KEYLOC(n)` |
| `F` file condition 71-72 | `EXTIND(*INUx)` |
| `F` `SPECIAL` routine and entry | `PGMNAME('x')`, `PLIST(x)` |
| `F` continuation `K` lines | `RENAME(ext:new)`, `IGNORE`, `INCLUDE`, `INFDS`, `INFSR`, `SFILE(fmt:rrn)`, `COMMIT`, `PLIST`, `DEVID`, `SLN`, `PRTCTL`, `FMTS`, `SAVEDS`, `SAVEIND`, `MAXDEV`, each on its own F line |
| `L` form length and overflow line | `FORMLEN(n)`, `FORMOFL(n)` on the file's F line; the L record is dropped |
| `E` array or table | `D name S len[P/B] dec DIM(n) [CTDATA \| FROMFILE(f)] [TOFILE(f)] [PERRCD(n)] [ASCEND \| DESCEND]`; the alternating array a second `D` with `DIM(n) ALT(name)` |
| `I` data structure (`DS` in 19-20) | `D name DS [len]`, `E` and `EXTNAME(f)` for 17 `E`, `S`/`U` in 23 for a status or data area structure, `INZ` for 18 `I`, `OCCURS(n)` |
| `I` subfield | `D  name from to [P/B] dec`, special keywords (`*STATUS`, `*FILE`) in the from columns, `INZ(value)` for column 8 `I`; in an external structure `E` and `EXTFLD(old)` |
| `I` named constant (`C` in 43) | `D name C CONST(value)` |
| `I` record and field lines | the same entries at the ILE RPG columns |
| `C` 18-27, 28-32, 33-42, 43-48, 49-52, 54-59 | 12-25, 26-35, 36-49, 50-63, 64-70, 71-76; comments 60-74 to 81 |
| `C` half adjust (53) | the operation extender `(H)`, and `(P)`, `(N)` |
| `C` more than one conditioning indicator | a line per indicator, the later ones `AN` |
| operations | `BITOF` `BITOFF`, `CHEKR` `CHECKR`, `COMIT` `COMMIT`, `DEFN` `DEFINE`, `DELET` `DELETE`, `EXCPT` `EXCEPT`, `LOKUP` `LOOKUP`, `OCUR` `OCCUR`, `REDPE` `READPE`, `RETRN` `RETURN`, `SELEC` `SELECT`, `SETOF` `SETOFF`, `UNLCK` `UNLOCK`, `UPDAT` `UPDATE`, `WHxx` `WHENxx` |
| `ARR,I`, `*IN,nn` | `ARR(I)`, `*IN(nn)` |
| `O` record line 15-22, 23-31, 32-37 | 17-20, space and skip at 40-51, 21-29, 30-39 |
| `O` field line 23-44, 45-70 | 21-29, 30-43, 44, 45, 47-51, 52, 53-80 |

`tools/conformance/tests/rpg3_cvt.rs` compares each corpus member's
conversion with the member pub400's `CVTRPGSRC` wrote, record for
record, and the conversion report with pub400's (dates, the user and
the library masked, each message's text its own). The report is IBM's
layout: the command's parameters, the files, then (when there are
messages) each record with a message as converted, under its sequence
number, with its messages, and the message summary on a page of its
own; the final summary of message and record totals.

The converter's messages: `RNM0511` (00) a `CALL`, whose program may
need converting too; `RNM0506` and `RNM0517` (30) `FREE` and `DEBUG`,
which ILE RPG does not have and which are copied as they are (IBM's
numbers and severities). QS/400's own, for what the corpus has not
shown IBM's converter doing: `RNM0590` an unknown specification type,
`RNM0591` a data format `L` or `R` on an extension specification,
`RNM0592` a file continuation option it does not know. Only those three
make `CRTRPGPGM` fail; `FREE` and `DEBUG` compile and do nothing
(`DEBUG` writes no debug records), where RPG/400 runs them.

## Commands

`CRTRPGPGM PGM(lib/name) SRCFILE(lib/QRPGSRC) SRCMBR(*PGM) GENLVL(9)
TEXT OPTION GENOPT ... REPLACE TGTRLS USRPRF` and `CVTRPGSRC FROMFILE
FROMMBR(name|*ALL) TOFILE(lib/QRPGLESRC) TOMBR(*FROMMBR) EXPCPY CVTRPT
SECLVL INSRTPL LOGFILE LOGMBR` run in the storage service
(`lic/slic/src/cpp/rpg/rpg3.rs`), refused unless `QS00RPG` is
installed. `CVTRPGSRC` writes members of source type `RPGLE` and
refuses a target member that exists; `EXPCPY(*YES)` and the log file
are accepted and have no effect.

## Conformance

Batch 22 (`tools/conformance/rpg/b22rpg3/*.rpg`, written by
`gen/rpg3.py`): RPG III members compiled with `CRTRPGPGM` and run on
pub400 (V7R5M0, 5770WDS with RPG/400) and in the kit; `rpg-oracle`
uploads an `.rpg` member to `QRPGSRC` (92 bytes, source type `RPG`),
compiles it, calls it, and converts it with `CVTRPGSRC` into
`QRPGLESRC`, keeping `NAME.out`, `NAME.lst`, `NAME.cvt` (the converted
member) and `NAME.rpt` (the report). `cargo test -p conformance --test
rpg_kit` (`QS400_RPG_ONLY=R30`) compares the outcomes,
`--test rpg3_cvt` the conversions and reports.

### Results

25 members (`R3001`-`R3025`): edit codes and words, arithmetic and half
adjust, comparisons and the structured operations, moves, arrays and
tables with compile-time data and alternating tables, strings, data
structures with initial values, constants and occurrences, the program
status structure and `*ENTRY` parameters, indicators with `AND`/`OR`
conditioning, an HLINV-style report with `L2`/`L1` breaks and totals
(`R3011`), matching records (`R3012`), externally described input,
update, add and delete, subroutines and `*INZSR`, a line counter
specification with overflow and headings again, record identification
codes, `KLIST`, an externally described data structure with renames,
bit operations, file continuation lines with an INFDS, an externally
described primary file with a control level, `DEFN` with a data area,
signs and zones, and one compile with errors. All 25 conversions and
reports match pub400's; 24 outcomes match, one is known to differ
(`known.txt`):

- `R3021`: the INFDS's `*RECORD` names the file's own format (`CUSTR`)
  where RPG/400 gives the `RENAME` name (`CREC`), and keeps it after a
  `CHAIN` that finds nothing where RPG/400 blanks it. The IR's file
  declaration carries no RPG names for formats.

Fixes the corpus brought to the ILE RPG compiler and runtime, which
ILE RPG programs share: an output field with an edit code and `*` or
the currency symbol in the constant columns (asterisk fill, floating
currency) printed nothing; a whole array named on an output
specification printed one element (now every element, edited ones two
blanks apart, the blanks printed too); the PSDS's `*PARMS` was 0 in a
cycle program called with parameters; an externally described field
line with a control level or matching field and no rename was not
recognised.

**Overflow.** RPG/400 spaces after a line when the next one prints, so
its overflow indicator goes on when a line prints on the overflow line;
ILE RPG spaces at once and the indicator goes on when the carriage
reaches the line (`COV096` and `R3016` on pub400 show both). `CRTRPGPGM`
compiles `FORMOFL(n)` (the line counter specification's overflow line)
as `FORMOFL(n+1)`, which overflows at the same line. A program without
a line counter specification keeps the printer file's overflow line and
the ILE timing.
