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

**Database files.** A member's `dds: NAME` directive creates
`NAME` from `rpg/dds/NAME.pf` (or `.lf`) in the work library before
the compile (the source goes into a `QDDSSRC` there, `CRTPF`/`CRTLF`,
the member removed); `sql: STATEMENT` and `sqlfile: NAME` (the
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

## Checks not yet made

- The compile-time checks above (`RNF7021`, `RNF7262`, `RNF7531`,
  `RNF7017`, `RNF7565`, `RNF7143`, `RNF5216`, `RNF0592`, the
  never-set indicator) are not made yet; the corpus keeps to valid
  programs.

## Blocked

- **The primary oracle has no RPG compiler.** Every compile-and-run
  result is captured on pub400 (V7R5M0) instead; nothing in the corpus
  is verified on V7R1M0.
- **HLINV is on neither oracle.** No user library on Cody's box holds
  a source file, an RPG object or anything named `HLINV`, and pub400's
  account has none either. The HLINV harness skips with a message
  until its source is placed in `tools/conformance/hlinv/src/`.
