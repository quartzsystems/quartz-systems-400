# Phase 9: the ILE RPG compiler

Phase 9 adds the ILE RPG compiler (`CRTRPGMOD`, `CRTBNDRPG`,
`CRTSQLRPGI`), its runtime (`qrpgrt`) and the `RNF`/`RNQ`/`RNX`
message files. The method is differential: each corpus member is
compiled and run on an IBM i and on QS/400 and the normalized outcomes
are compared. `docs/RPG.md` describes the compiler.

## Oracle systems

Recorded on first connection, 2026-10-23.

| System | Release | RPG compiler | Reached through |
|---|---|---|---|
| Cody's AS/400 | V7R1M0 | none: `CRTBNDRPG` and `CRTSQLRPGI` are not in `QSYS` (5770-WDS not installed); `QRNXMSG` is there | host servers only (JTOpen command server, JDBC); no FTP, no telnet |
| pub400.com | V7R5M0, 5770-WDS V7R5M0 (`220415` in the listing heading) | ILE RPG and the SQL precompiler | FTP (`RCMD`), JDBC, telnet |

Since only pub400 compiles RPG, every expected result in
`tools/conformance/rpg/expected/` comes from pub400, V7R5M0, and QS/400
targets that release (`docs/RPG.md`, target release). The primary
oracle is still used where it can answer without a compiler.

## The `rpg-oracle` tool

`cargo run -p rpg-oracle -- run MEMBER.rpgle...` (or `refresh DIR
[--missing]`, `clean`) opens one FTP session with pub400 and, per
member:

1. creates `QRPGLESRC` (CCSID 37, the member's record length) and a
   spool copy file in the account's `B` library;
2. uploads the member as binary data records; the server numbers the
   lines `1.00`, `2.00`, ... with zero change dates, which QS/400's
   kit reproduces;
3. runs the member's compile command (`//; cmd:`, default `CRTBNDRPG
   ... DBGVIEW(*NONE) OUTPUT(*PRINT)`), at least 10 seconds after the
   previous compile;
4. calls the program (`//; call:`, or `//; run: no` to compile only)
   between two marker messages;
5. finds the spooled files of the compile and of the call (they land
   in the account's `QPRTJOB` job) with a query of
   `QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC`, copies each with `CPYSPLF ...
   CTLCHAR(*FCFC)` and deletes it; reads the job log between the
   markers from `QSYS2.JOBLOG_INFO('*')`;
6. deletes the program, module or service program and the member, and
   at the end of the session the work files.

One session is open at a time on this machine: sessions take a lock
in the system temporary folder (`qs400-rpg-oracle/session.lock`), where
the last compile's time is kept too, so parallel worktrees share the
compile gap.

**Database, display and printer files.** A member's `dds: NAME`
directive creates `NAME` from `rpg/dds/NAME.pf` (or `.lf`, `.dspf`,
`.prtf`) in the work library before the compile (the source goes into
a `QDDSSRC` there, `CRTPF`/`CRTLF`/`CRTDSPF`/`CRTPRTF`, the member
removed; a failing create reports its `CPD` messages); `sql: STATEMENT` and `sqlfile: NAME` (the
statements of `rpg/dds/NAME.sql`) run through `RUNSQLSTM` with `{lib}`
the work library, a run of statements in one member. Every file a step
creates is deleted after the run, the last first. The work library is
added to the job's library list when the session opens, so the
compile and the call find the files through `*LIBL`. The kit builds
the same files in memory: the DDS through the `dds` compilers and
`db::file`, the statements through the Phase 3 SQL engine, with `QGPL`
as `{lib}`; the compiler reads the formats from those `*FILE` objects
(`conformance::rpg::files::KitExt`, an `rpgc::ExtFiles`) and the job's
`StoreIo` runs over the same store. A failing `RUNSQLSTM` reports the
`SQL` lines of its listing. Statements are cut into lines at blanks,
so a literal must not hold two blanks in a row.

**Display files.** A member with `keys:` lines is called at a display:
`rpg-oracle` signs on to pub400 through the 5250 driver
(`tn5250-driver`, automatic sign-on with the account from the `.env`
file, one session per run), adds the work library to the job's list,
sets `CHGJOB INQMSGRPY(*DFT) DECFMT(*BLANK)` and types the call on the
command line. Each screen the program reads is kept as its text rows,
and the member's next `keys:` step is played on the terminal (fields
typed with or without Field Exit, the cursor placed, the key pressed);
when the steps run out the session waits for the command line again,
and a program still reading is ended with System Request 2 (`CPF1907`).
The kit plays the same steps on the same `tn5250` terminal over the
QS/400 display-file runtime (`conformance::rpg::screen`), its display
files compiled by `dspf`, so both sides read their screens the same
way. The outcome's `screen: n` sections hold the rows. The FTP job runs
with `DECFMT(*BLANK)` too, so printer files edit as the kit's do.

Queries run as `RUNSQLSTM` over an uploaded member because an `RCMD`
line holds only about 250 characters; their tables have fixed-width
CP037 columns and are read back in binary. The job runs with
`INQMSGRPY(*DFT)`, so an unhandled RPG inquiry takes its default reply
instead of reaching the system operator.

The outcome (`NAME.out`) holds whether the object was created, the
compile command's failure message, every `RNF` diagnostic (identifier,
severity, statement, sequence), how the call ended, the spooled files
as `page/line text` rows of printed lines, and the job log messages
(identifier, type, replacement data). pub400's `QSYSPRT` pads each
page with blank lines up to a footer naming the site; blank lines and
the footer are not part of the comparison. The masked listing goes
beside it (`NAME.lst`): page headings, dates, times, the library, user
and system names are placeholders.

## Oracle facts: files and the cycle

Recorded from batches 6 and 7 (`b06cycle`, `b07io`), pub400 V7R5M0.

- The listing numbers generated lines like source lines: an `EXTNAME`
  structure's subfields (`=D`) after its line, the input
  specifications of externally described input formats (`=I`: a record
  line unless an I spec names the format, then one line per field)
  before the first calculation, the output specifications (`=O`) after
  the main calculations. Diagnostic line numbers count them.
- A search argument that is not a list must have the first key
  field's length and decimals: `CHAIN 999 F` against a `5P 0` key is
  `RNF7078` (30); `(999)` and `%KDS` convert.
- Without `EXTDESC` the compile-time description is the file name of
  the F spec, not `EXTFILE`'s (`RNF2120`, 40, otherwise).
- `RNF7066` (00): a record format no operation names (file-name I/O does
  not count; primary and secondary files are exempt). `RNF7086` (00):
  an input-only file read only by `READ` (and primary files) is
  blocked. `RNF6011` (00): an output record without space and skip
  entries. A blank field name on an external I-spec field line is
  `RNF4050`.
- `%STATUS(file)` after `CHAIN(E)` that finds nothing is 00012 with
  `%ERROR` off; `OPEN` of an open file is 01215; `UPDATE` after a read
  without a lock (`(N)`, `UNLOCK`) is 01221; `WRITE` of a duplicate key
  01021; a closed file 01211. `CLOSE *ALL` closes the printer too.
- The INFDS operation code is five characters and `F` (by file name) or
  `R` (by format): `READ F`, `CHAINR`, `WRITER`, `DELETF`.
- After `FEOD` on an input file the next `READ` is at end of file.
- A program-described printer's first line with space-before `n`
  prints on line `1 + n`.
- In the cycle, total calculations and output are skipped on the first
  cycle; `L1`-`L9` set at a break stay on through the detail output of
  the next cycle; overflow lines print after total output, then the
  indicator goes off; fetch overflow prints them before the record.

## Oracle facts: display files

Recorded from batch 8 (`b08dsp/`), pub400 V7R5M0.

- A display format's input buffer starts with its response indicators
  in the order they first appear, file-level keys first; the output
  buffer with the option indicators the same way. A subfile record's
  input buffer holds its output-only fields: `CHAIN` and `READC` give
  them back. With `INDARA` the RPG listing has no indicator lines and
  notes `RNF7089` (00) at the file.
- The 5250 start of header carries the `CAnn` keys as command key
  switches (a bit on for no data); a terminal that honours them sends
  no fields, and the program's fields stay as they were.
- `ERRMSG` on a record already shown sends only the message: the
  operator's typing stays on the screen; only the first message in
  effect shows.
- A numeric-only (`Y`) input field without an edit code takes its
  length on the display; typed digits without a decimal point are
  whole units (`1250` in a `7Y 2` field is 1250.00).
- Edit codes in display and printer files follow the job's decimal
  format (pub400's default is the comma); an unedited `PAGNBR` prints
  four digits (`0001`).
- A window's border is at its `WINDOW` position, its inside starts two
  columns in, and it is `cols + 4` wide.
- `SFLEND` needs an option indicator (`CPD7490`); a control record with
  fields above and below its subfile's page overlaps it (`CPD7812`).
- `NEXT` is an operation code: `next += 1;` in free form does not
  compile (`RNF5309`).

## Intentional differences

- Format level identifiers are QS/400's (`DB_FORMAT.md`), not IBM's;
  only the check at open uses them.
- An I/O error with neither `(E)` nor an error indicator does not yet
  reach the file's exception handling (`INFSR`, the default handler's
  inquiry, `CEE9901` at the call): the operation's status is kept and
  the program goes on. `TODO(phase9)` in `codegen/dbio.rs`.
## Oracle facts: moves, strings and arrays

Recorded from batch 5 (`b05move/`) and the `STR`, `ARR` and `BIT`
members of batch 4, pub400 V7R5M0.

- `MOVE` and `MOVEL` treat every number as its zoned digits, the sign
  in the last zone, whatever its storage (packed, binary, integer);
  a positive number moves with the `F` zone, so it reads as digits in
  character data. Decimal positions take no part.
- A numeric target takes the digit half of every byte and the sign of
  its last zone. `MOVEL` into a number that factor 2 fills or overfills
  takes factor 2's sign from its last position, even when that position
  is not moved; a shorter factor 2 leaves the target's sign.
- Character data whose digit halves are not decimal digits (`+ - . ,`)
  moved into a number is a decimal data error at run time; blanks move
  as zeros.
- A varying target keeps its current length under `MOVE`, `MOVEL` and
  `CAT`, with or without `(P)`; `MOVE` of a varying source moves its
  current value.
- `MOVE` of character data into an indicator is `RNF7531`; a structure
  moved into its own subfield is `RNF7017`; `MOVEA` between numeric
  arrays of different type or length is `RNF7262`; `EVALR` into a
  varying field is `RNF7565`; `SUBST` with a constant length or start
  past the base is `RNF7143` / `RNF5216`.
- `LOOKUP`'s search argument must have the element's type and length,
  literals included (`RNF7021`): `25` does not search a `3P 0` array.
- `%LOOKUPLT`, `LE`, `GT`, `GE` need an `ASCEND` or `DESCEND` array
  (`RNF0592`).
- An indicator `*INxx` that nothing sets (no `SETON`, resulting
  indicator or assignment) is `RNF7030` where an expression uses it.
- 7.5 accepts `%SCANRPL`, `%SCANR`, `%CONCAT`, `%CONCATARR`, `%SPLIT`
  (with `FOR-EACH` and array assignment), `%LEFT`, `%RIGHT`,
  `%CHARCOUNT`, `%MAXARR`, `%MINARR`, `IN %LIST(...)`, `IN %RANGE(...)`,
  `DIM(*AUTO)` and `DIM(*VAR)` with `%ELEM` as a target and `*NEXT`,
  the `%BITxxx` functions, `%STR` and `SORTA %SUBARR`.

## Oracle facts: embedded SQL

Recorded from batch 11 (`b11sql/`), pub400 V7R5M0.

- `CRTSQLRPGI ... OUTPUT(*PRINT)` prints the precompile listing and
  then the compiler's; the tool keeps both, in that order. The
  precompile diagnostics are `SQLnnnn sev record`; a precompile above
  `GENLVL` ends with `SQL9001` and no compile; a compile that fails is
  `RNS9310`.
- The compile listing numbers the generated member's lines: the SQLCA
  block (65 lines) and the main statements' structures go before the
  first calculation, a procedure's structures before its first
  calculation, the call code after each statement (`lines.rs` has the
  counts per kind). Sequence numbers stay the source's.
- `SELECT INTO` of no row is `+100 02000`, `SQLERRD(3)` 0; of several
  rows `-811 21000` with the first row assigned. A null without an
  indicator is `-305 22002`; with one the variable is emptied (zero).
  A number too large for its host variable is `-304 22003`, nothing
  assigned; decimals are truncated. `'Hello'` into `char(3)` is `Hel`,
  `01004`, indicator 5, `SQLWN0` and `SQLWN1` `W`.
- An `UPDATE` or `DELETE` that changes nothing is `+100`; a column
  function over nulls is `01003` with SQLCODE 0.
- Cursors: `FETCH` or `CLOSE` of a cursor not open is `-501 24501`,
  `OPEN` of an open one `-502 24502`, `WHERE CURRENT OF` a closed one
  `-507 24501`. `FETCH ABSOLUTE` is `SQL0104` at precompile time on
  7.5. A multiple-row `FETCH` that reaches the end gives `SQLERRD(5)`
  100.
- `COMMIT` and `ROLLBACK` under `COMMIT(*NONE)` are `-7007 51009` and
  close nothing.
- `PREPARE` reports `-104` and `-204`; `EXECUTE` of a statement whose
  `PREPARE` failed is `-518 07003`.
- `GET DIAGNOSTICS ... NUMBER` is 1 after a statement that worked.
- `SET OPTION` gives `SQL5066` (00) per option; a missing table is
  `SQL1103` (10) and `-204 42704` at run time; `DELETE` without `WHERE`
  is `SQL0088` (00).
- pub400's jobs run with CCSID 273: `CHAR` of a decimal has a comma.
  The corpus keeps to integers there.
## Oracle facts: the compile listing

Recorded from the 350 listings of batches 1-12, pub400 V7R5M0. The
listing is compared line for line by `tests/rpg_listing.rs` after one
normalization of both sides (`conformance::rpg::lstcmp`): the heading,
user, library, dates and times are placeholders, pub400's page padding
goes, and every message text is `{text}` with its continuation lines
dropped (ours are Quartz-written, so they differ by rule); the
completion line keeps only its severity. `known_lst.txt` names the
members known to differ.

- **Pages.** The overflow line is 60: a record whose spacing would pass
  it starts a page; a section's end line may print on line 61, and a
  page left at or past 60 gets an empty overflow page (with the source
  rulers in the source section) before the next section. After an
  overflow heading the first record spaces one line more, except in
  the source section, where it is single-spaced.
- **Source records.** Free-form source prints columns 1-100, fixed
  form columns 6-100 under its own rulers (a fixed-form copy member or
  an `=D` block switches the rulers with two `-` records, and back).
  A change of specification type (H F D I C O P; comments and
  directives do not count; `END-PROC` only outside a procedure)
  spaces three lines; when the first record would, a blank `-` record
  comes first. Unused resulting-indicator positions of a fixed
  calculation print as `--`. "Do Num" is `Bnn`, `nn`, `Xnn`, `Enn`.
  Lines conditional compilation leaves out are not numbered; with
  `*NOSHOWSKP` each run is one `LINES EXCLUDED: n` record.
- **Diagnostics.** A statement's diagnostics print after its last line
  (fixed form: before the next statement), those of a data structure
  or prototype after its group (`RNF3530`, `RNF7301`), `RNF2318`
  before the first input, calculation or procedure specification, and
  the RNF7xxx checks (with `RNF0331`, `RNF0334`, `RNF0362`, `RNF0599`,
  `RNF5276`) in the additional diagnostics. One on the line just
  printed gets a letter and a `======>` marker under its token
  (`RNF5005`, `RNF0256`, `RNF5177`, `RNF5178`, `RNF7301` never do).
  Each identifier's text takes a fixed number of lines
  (`listing/lines.rs`); ours are wrapped to the same.
- **Generated lines** (`*EXPDDS`): a box after each externally described
  file; `=D` subfields after an `EXTNAME` structure (a `LIKEREC` one
  only switches the rulers); `=I` before the first calculation, `=O`
  after the main calculations for formats an output operation names.
  Each block (and each copy member) is a source with its own id and
  sequence numbers. Key field information, the output buffer
  positions (once per kind of output operation), the match field
  table and the `/COPY` members are their own sections or boxes.
- **Cross-reference.** Names in EBCDIC order (indicators: `*IN`,
  numbered, L1-L9, LR, overflow, MR, 1P), each reference on the line
  its token is on, `M` where the statement changes it (a bare call of
  a procedure is `M`, `CALLP` is not; a `FOR` index is not; `TESTB`
  and `PARM` results are), unreferenced names `RNF7031` (counted in the
  summaries, indicators excepted), fields of files and input
  specifications packed, qualified subfields under their structure.
  The message summary is in severity, then identifier order.

## Checks not yet made

- The compile-time checks above (`RNF7021`, `RNF7262`, `RNF7531`,
  `RNF7017`, `RNF7565`, `RNF7143`, `RNF5216`, `RNF0592`, the
  never-set indicator) are not made yet; the corpus keeps to valid
  programs.

## Embedded SQL not yet done

- `CLOSQLCSR(*ENDMOD)` is accepted but cursors live until the job (or
  the kit's call) ends, as `*ENDACTGRP` would leave them while the
  activation group lives. `TODO(phase9)`.
- Commitment control: `COMMIT(*CHG)` and the others do not start it;
  the corpus compiles with `COMMIT(*NONE)` (pub400's work library is
  not journaled).
- `DESCRIBE`, SQL descriptors and `INCLUDE` of members are not
  supported; `DESCRIBE` reaches the runtime as `-84`.

## Blocked

- **The primary oracle has no RPG compiler.** Every compile-and-run
  result is captured on pub400 (V7R5M0) instead; nothing in the corpus
  is verified on V7R1M0.
- **HLINV is on neither oracle.** No user library on Cody's box holds
  a source file, an RPG object or anything named `HLINV`, and pub400's
  account has none either. The HLINV harness skips with a message
  until its source is placed in `tools/conformance/hlinv/src/`.

## Intentional differences

- **`*JOBRUN`** is the job's date and time format on IBM i. QS/400
  jobs have no date format yet, so `*JOBRUN` is `*MDY/` for dates and
  `*HMS:` for times, and `TIME` into a 12- or 14-digit field appends
  the date as `*MDY` (`mmddyy`, `mmddyyyy`). The corpus tests these
  only by round trip, never by printing them.
  until its source is placed in `tools/conformance/hlinv/src/`. With
  the source there, the harness builds the application's DDS
  (`QDDSSRC/`: physical and logical, display and printer files) in
  memory for the compiler; `ITEMINQ` and `INVMENU` compile clean.
  Still failing: the printer files use data type `Y`, which `CRTPRTF`
  refuses on pub400 too (`CPD7419`), so `STKRPT` and `REORDRPT` miss
  their printer formats; `%DATE()` and `%TIME()` (`ITEMMNT`,
  `TRNPOST`) and the data area operations `IN`/`OUT` (`TRNPOST`) are
  not compiled yet.
