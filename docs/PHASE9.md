# Phase 9: the ILE RPG compiler

Phase 9 adds the ILE RPG compiler (`CRTRPGMOD`, `CRTBNDRPG`,
`CRTSQLRPGI`), its runtime (`qrpgrt`) and the `RNF`/`RNQ`/`RNX`
message files. The method is differential: each corpus member is
compiled and run on an IBM i and on QS/400 and the normalized outcomes
are compared. `docs/RPG.md` describes the compiler.

## What landed

- **The compiler** (`rpg/front`, `rpg/compiler`, `rpg/specs`,
  `rpg/sqlpre`): fixed form, column-8 free form and `**FREE`; every
  specification, directive and continuation form; declarations of
  every type, data structure form and keyword; IBM's intermediate
  precision rules; every operation code and built-in function of 7.5
  (`gen/matrix.py` lists them, at the end of this document); the
  cycle; database, display and printer files; subprocedures,
  prototypes, service programs and binder language; embedded SQL
  (`CRTSQLRPGI`); the compile listing in IBM's layout; debug views.
- **The runtime** (`rpg/qrpgrt`, with `runtime/mi`): moves, edit codes,
  dates, strings, arrays, XML, the PSDS and INFDS, exceptions
  (`MONITOR`, `*PSSR`, `INFSR`, the default handler and its `RNQ`/`RNX`
  messages), commitment control, record locks.
- **Commands and tools**: `CRTRPGMOD`, `CRTBNDRPG`, `CRTSQLRPGI` in the
  storage service (`lic/slic/src/cpp/rpg/`); the `RNF`/`RNQ`/`RNX`
  message files with Quartz-written texts; SEU's RPG syntax check, PDM
  options, `STRDBG` over the debug views, `DSPSFWRSC`.
- **Harnesses**: the `rpg-oracle` tool, the corpus kit, the listing
  comparison, the grammar fuzzer, HLINV, the open-source corpus, the
  bench, the layout and IBM-text checks, and QEMU steps 56-58.

## Corpus

Every member is compiled and run on pub400 (V7R5M0) and in memory on
QS/400; the normalized outcomes (diagnostics, created or not, the run,
spooled files, job log, screens) must be equal
(`cargo test -p conformance --test rpg_kit`). The listings are
compared line for line by `tests/rpg_listing.rs`.

| Batch | Folder | Members | Known to differ |
|---|---|---|---|
| 1 Specifications | `b01spec` | 150 | 0 |
| 2 Declarations | `b02dcl` | 200 | 0 |
| 3 Expressions (about 5,000 EVAL cases) | `b03expr` | 127 | 1 |
| 4 Operations and built-ins | `b04ops` | 74 | 0 |
| 5 Moves | `b05move` | 22 | 0 |
| 6 The cycle | `b06cycle` | 13 | 0 |
| 7 File I/O | `b07io` | 37 | 0 |
| 8 Display files | `b08dsp` | 22 | 0 |
| 9 Exceptions | `b09exc` | 44 | 0 |
| 10 Procedures | `b10proc` | 12 | 0 |
| 11 Embedded SQL | `b11sql` | 55 | 0 |
| 12 Compile errors | `b12err` | 370 | 22 |
| 13 Listings | `b13lst` | 40 | 0 |
| 14 Fuzzer samples and bundles | `b14fuzz` | 140 | 0 |
| 15 Coverage of every operation, function and keyword | `b15cov` | 134 | 1 |
| 16 IR types (long varying, strided arrays, file parameters, nulls) | `b16ir` | 8 | 0 |

Known differences (`known.txt`, each with its reason): `EXP118` (a
float operation with a one-decimal operand: IBM converts that decimal
to the nearest double where longer ones go toward zero; open
question), `COV131` (pub400's CCSID 273 job cannot convert to CCSID
835), and 22 compile-error members whose only difference is IBM's
follow-on diagnostics after `RNF3556` or an extra `RNF5410`.

Listings that differ only in layout are named in `known_lst.txt`;
their outcomes match.

## Harnesses

- **Fuzzer** (`tools/rpg-oracle/src/fuzz.rs`): random valid free-form
  programs. `cargo test -p conformance --test rpg_fuzz` compiles and
  runs 2,000 (release) and the same 2,000 as 100 bundles of 20.
  `rpg-oracle fuzz --count 2000` wrote those bundles
  (`b14fuzz/FZB000`-`FZB099`) and ran them on pub400: all 2,000
  programs match, after fixes for the line of `RNF7551`, a quotient
  set kept across procedures, and unsigned `%ABS`.
- **HLINV** (`tests/hlinv.rs`, source in the gitignored
  `tools/conformance/hlinv/src/`): 4 of 6 programs compile clean and
  match IBM's listings; `STKRPT` and `REORDRPT` use printer DDS with
  data type `Y`, which `CRTPRTF` refuses on pub400 as well, so they are
  left out (`hlinv/known.txt`). The source is unmodified.
- **Open-source corpus** (`tests/oss_corpus.rs`, sources in the
  gitignored `tools/conformance/oss/src/`: noxDB, ILEastic,
  ibmi-company_system): every member compiles without a compiler
  failure; 181 of 303 are created, the rest need objects, copy
  members or IFS files the projects do not hold.
- **Layouts** (`tests/rpg_layouts.rs`): the INFDS and PSDS tables of
  `RPG.md` against the code.
- **IBM text** (`tests/phase9_hygiene.rs`, `tests/phase8_hygiene.rs`):
  no IBM message text in `rpg/`, `qrpgrt` or the message sources; no
  credential or account name in a tracked file.
- **QEMU** steps 56 (`CRTBNDRPG` and a call), 57 (`CRTSQLRPGI`) and 58
  (`CRTRPGMOD`, `CRTSRVPGM`, `CRTPGM BNDSRVPGM`).

## Bench

`cargo test --release -p conformance --test rpg_bench -- --ignored
--nocapture` on the development PC (Windows, release build); the same
members (`rpg/perf/`) timed themselves on pub400, an IBM Power model
22A (processor feature ERGR, 2 processors, V7R5M0, a shared public
system).

| | QS/400 | pub400 |
|---|---|---|
| Compile, front end | 300,000 lines/s | |
| Compile, to IR | 51,000 lines/s | |
| 1,000,000 decimal EVAL pairs (`PERF01`) | 570 ms | 876 ms |
| 100,000 subprocedure calls (`PERF01`) | 46 ms | 2 ms |
| 100,000 keyed `CHAIN`s over 10,000 records (`PERF02`) | 231 ms | 541 ms |
| A cycle report over 10,000 records, a break per 100 (`PERF03`) | 16 ms | 6 ms |

## Phase 9b

- RPG III and RPG/400 (`CRTRPGPGM`, `RPG` and `RPG36` source types) and
  `CVTRPGSRC`.
- `DATA-INTO`/`DATA-GEN` parsers and generators, Open Access handlers
  and the `DUMP(A)` layout (under "Blocked": they need QS/400's own
  `QOAR` copy members and a dump writer).
- Job date and time formats behind `*JOBRUN` (work management).
- The listings named in `known_lst.txt` (layout only; their outcomes
  match) and the remaining items of "Not done" below.

## Acceptance

| # | Criterion | Status |
|---|---|---|
| 1 | Corpus passes against committed oracle results, differences listed with reasons | Pass (0 differ; 24 known, listed above) |
| 1 | `cargo run -p build -- test` passes | Pass (see the QEMU run below) |
| 1 | Fuzzer run complete with no unresolved differences | Pass (2,000 on pub400) |
| 2 | HLINV compiles unmodified; batch, interactive and data diffs clean | Needs Cody: two printer files use data type `Y`, which IBM refuses too; no recorded keystroke scripts or oracle data exist for the interactive and data diffs |
| 3 | `RPG.md` and this document: front end, tables, rules, precision, cycle, exceptions, runtime, SQL, release mapping; feature matrix; bench; fuzzer; open-source results; deferred list | Pass |
| 3 | Spec-versus-code test for the INFDS, PSDS and listing layouts | Pass (`rpg_layouts`; the listing layout through `rpg_listing`) |
| 4 | `missing_docs = deny` on the RPG crates and `qrpgrt`; clippy and `cargo deny` clean; static binaries | Pass |
| 5 | No IBM text in the message files, listings or help; tripwire covers `rpg/`, `qrpgrt/`, `msgs/` | Pass |
| 5 | Oracle artifacts only under `conformance/`; `.env` and `hlinv/src/` untracked; no credential tracked | Pass in the tree; needs Cody: commit f994a61 holds the pub400 account's library name in two outcomes (removed since; history is not rewritten here) |
| 5 | `QSTEST` and the pub400 library empty of this phase's objects | Needs Cody to confirm (`rpg-oracle clean` run at the end) |

## Intentional differences

- Format level identifiers are QS/400's (`DB_FORMAT.md`), not IBM's;
  only the check at open uses them.
- An unhandled file error ends the program through the function check
  and `CEE9901`, the outcome IBM's default reply gives; the `RNQ1xxx`
  inquiry itself is not sent (no job log is compared).
- A record another job holds is waited for `WAITRCD` long by the job
  process (`stg::remote::RemoteIo`); the kit runs one job, so no
  member waits. Records read under `LCKLVL(*ALL)` are locked only by
  paths opened for update, as the oracle shows within one job; a
  second job's input path is not refused.
- **`*JOBRUN`** is the job's date and time format on IBM i. QS/400
  jobs have no date format yet, so `*JOBRUN` is `*MDY/` for dates and
  `*HMS:` for times, and `TIME` into a 12- or 14-digit field appends
  the date as `*MDY` (`mmddyy`, `mmddyyyy`). The corpus tests these
  only by round trip, never by printing them.

## Not done

**Checks.**

- The follow-on diagnostics of a group child read as an operation
  (`RNF3556`) are whatever the calculation gives here; some members
  differ in them (`known.txt`).

**Embedded SQL.**

- `CLOSQLCSR(*ENDMOD)` ends the module's statements when the program
  entry returns normally; an escape past it leaves them until the
  activation group ends. A `NOMAIN` module's exported procedures do
  not end them yet (a service program's cursors live with its group).
- Commitment control (`SQL050`-`SQL055`): an activation group's commit
  definition is not committed when the group ends normally (IBM's
  implicit commit); the corpus ends its units of work itself.
- `DESCRIBE`: the date length is the `*MDY` job format's (8) whatever
  `DATFMT`; `USING NAMES|LABELS|ANY|BOTH`, `SQLVAR2` entries (LOBs,
  distinct types) and `DESCRIBE TABLE` and `DESCRIBE CURSOR` are not
  done. The SQLDA's generated declarations are not counted in the
  listing's line numbers, nor is `INCLUDE` of a stream file (`'path'`)
  read.

## Blocked

- **The primary oracle has no RPG compiler.** Every compile-and-run
  result is captured on pub400 (V7R5M0) instead; nothing in the corpus
  is verified on V7R1M0.
- **`DATA-INTO` and `DATA-GEN`** call no parser or generator: a parser
  is written to IBM's `QRNDTAINTO` interface and a generator to
  `QRNDTAGEN`, copy members in `QOAR/QRPGLESRC` that QS/400 does not
  have. Every one is status 355, as when the program cannot be called
  (`// BLOCKED(phase9):` in `codegen/xml.rs`).
- **Open Access** (`HANDLER`) handlers are not called; opening such a
  file ends the program as pub400 does when the handler is missing
  (`codegen/dbio.rs`).
- **`DUMP(A)`** writes no formatted dump (`_QRNX_DUMP`); pub400's holds
  the job and the dates, so the corpus does not print one.

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
3. runs the member's `//; setup:` commands, its `dds:`/`sql:` steps
   (below) and its `//; post:` commands (`CHGPF`, `STRJRNPF` on the
   files those steps made) and deletes the spooled
   files they leave (their listings are not the member's), then its
   compile command (`//; cmd:`, default `CRTBNDRPG
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

## Oracle facts: file exceptions

Recorded from the `FEX` members of batch 9 (`b09exc/`), pub400 V7R5M0.

- A file error with no `(E)`, error indicator, `MONITOR` or `INFSR`
  ends the program: the caller gets `CEE9901`. A `*PSSR` does not run
  for it (`FEX002`).
- `INFSR(subr)` runs with the file's INFDS filled (status, `UPDATR`,
  `WRITER`) and `%STATUS` the file's; `%ERROR` stays off. Its `ENDSR`
  with a blank return point or `*CANCL` ends the program as above;
  `*GETIN` and `*DETC` start the next cycle (no primary file).
  `INFSR(*PSSR)` runs `*PSSR`, whose PSDS status stays 00000 and
  routine `*DETC`.
- `MONITOR` takes a file error before `INFSR`. `ON-ERROR 1211`,
  `*FILE` and `*ALL` take file errors, `*PROGRAM` does not; the PSDS
  status stays 00000 but its exception identifier (40-46) is the
  `RNX` message of the status (`RNX1221`). After `ENDMON` `%STATUS` is
  0.
- A file error in a subprocedure reaches the caller's `MONITOR` as
  00202, identifier `RNX1021`; the procedure stops at the operation.
- An error in a subroutine reaches `INFSR` (`%STATUS` 01021) or
  `*PSSR` (00102) with its own status.
- A `WRITE` to a program-described printer that works makes `%STATUS`
  0: in a `*PSSR` that prints first, `IF %STATUS = 102` is false
  (`FEX012`; the first version of it looped until its spooled file
  outgrew the copy file).
- `OPEN` of a file that does not exist is 01217, `%OPEN` off.
- A `USROPN` file with no `OPEN` anywhere is `RNF7062` (30).
- The INFDS after an error taken by `(E)`: 38-45 the record format
  (`CUSTR`), 46-52 the data management message (`CPF5026` for a
  duplicate key, `CPF501B` for `UPDATE` without a read, blank for an
  operation on a closed file). The PSDS names the program and the
  module at 334-353 (`FEX013    FEX013`).
- The compile listing's file box names the `EXTDESC` file, not
  `EXTFILE`'s.
- A record lock within one job (two opens of one file, `FEX015`-`FEX018`,
  the file changed to `WAITRCD(1)` in a `post:` step): the second open's
  read for update fails at once with 01299, not 01218, and `CPF5032` in
  the INFDS (46-52), the record format there blank; `%ERROR` with
  `(E)`, the file's `INFSR` (`CHAINF`), `CEE9901` unhandled, and
  `ON-ERROR 1218` does not take it. A read without a lock (`(N)`)
  works; `UNLOCK` or `UPDATE` through the holding open frees the
  record. (01218 and `CPF5027` are another job's lock, after
  `WAITRCD`.) An earlier version of the members waited the default 60
  seconds and timed the FTP session out.
- A cycle program's `*PSSR` or `INFSR` (`EXC014`-`EXC027`) goes on at its
  `ENDSR` return point: `*DETL` the detail lines of the next cycle,
  `*GETIN` the next record (the detail lines skipped), `*TOTC` and
  `*TOTL` total time with the record that failed, then its fields and
  detail calculations again, `*OFL` the overflow lines and then the
  same, `*DETC` the detail calculations of the same record; `*CANCL`
  and blank end the program (`CEE9901`). After an error at total time
  `*GETIN` reads past the record already read for that cycle. The PSDS
  routine is `*DETC` for an error in the detail calculations, `*TOTC`
  at total time. `EXSR *PSSR` with `ENDSR '*GETIN'` goes to `*GETIN`
  too, not back after the `EXSR`.
- Fixed-form `D` specifications take a special word (`*ROUTINE`) across
  the From and To columns.
- Commitment control (`IO032`-`IO037`, the work library's file
  journaled in `post:` steps, `STRCMTCTL CMTSCOPE(*JOB)`): `ROLBK` undoes
  updates, writes and deletes since the last `COMMIT`; `COMMIT` with
  nothing pending is fine. Opening a `COMMIT` file without commitment
  control, or one that is not journaled, is 01217; `COMMIT` then is 802.
  `COMMIT(flag)` decides at the open. Under `LCKLVL(*CHG)` a record
  changed stays locked to the commit against an open of the job that is
  not under commitment control (01299), not against another open under
  the same definition; a record only read for update is freed by the
  next read. Under `*ALL` a record an input-only file read does not
  hold up an update open of the same job.

## Oracle facts: specifications and declarations

Recorded from batches 1 and 2 (`b01spec/`, `b02dcl/`), pub400 V7R5M0.

- Every `H` line continues the control keywords: a keyword's
  parameter list may go on onto the next line. With `OPTION(*SRCSTMT)`
  the listing numbers statements by their sequence numbers, and so do
  the diagnostics (the corpus avoids it).
- `DECEDIT` sets `%CHAR`'s point and, with `'0.'`/`'0,'`, a zero before
  it (`0.25`); `%EDITC` and output edit codes take its point and
  thousands separator. Edit code `Y` follows `DATEDIT`'s order and
  separator (`*YMD-`: `2026-12-25`, `26-12-25`).
- `/TITLE`, `/EJECT` and `/SPACE` lines are neither printed nor
  numbered. In a fully free member `/SPACE`, with or without a count,
  is `RNF0280` (20) and is listed.
- A `D`/`P` name continued with `...` is the parts joined as written.
  Comment and blank lines may sit between an `F` spec's continuation
  lines. A fixed-form `*FILE`, `*STATUS` keyword starts in column 26.
- A signed length with `LIKE` in the fixed-form length columns adjusts
  the length (`+3`); an unsigned one beside `LIKE` is `RNF3435`.
  `VARYING(4)` gives a 4-byte prefix.
- An `O` spec constant continues with `+` or `-` in the next line's
  columns 53-80. A field with blank-after prints first, then blanks.
  Records without spacing between them overprint: `CPYSPLF` keeps each
  pass a record of its own (`+` control), so the kit lists each pass.
- A full-procedural `READ` of a program-described file sets the record
  identifying indicator of the type read and leaves the others as
  they were.
- `TESTB` of one bit takes no "mixed" indicator (`RNF5117`).
  `ENDDO` factor 2 is a counted `DO`'s increment.
- In fixed-format source, text in columns 81-100 of a free-form line is
  `RNF5501` (20) inside a `/FREE` block; outside one only on the lines
  of a statement that spans lines.
- `**` with a blank in column 3 starts the next compile-time array's
  data; the rest of the record is a comment.
- `INZ(0)` of a field with no integer digits (`bindec(1:1)`) is
  `RNF3431`. A named constant `*ON` cannot be concatenated
  (`RNF7421`). A figurative constant (also through a named constant)
  initializes a varying field to its full length.
- `ALIGN` puts integer and float subfields on their size (2, 4, 8);
  binary decimals and varying prefixes are not aligned. Pointers are
  on 16 with or without it.
- An indicator takes no `INZ(*HIVAL)` or `*LOVAL` (`RNF3430`). `*USA0`
  is not a time format for `%CHAR` and `%TIME` (`RNF0599`). An
  indicator subfield of a structure without `INZ` is a blank until
  set; a workstation file's `INDDS` is all `0` once opened. `MOVEA(P)`
  into `*IN` pads with `0`.
- The open feedback in an `INFDS` holds the file name at 83, the record
  length (binary) at 125 and the member at 129. A static structure in
  a subprocedure takes its `INZ` values on the first call. A
  multiple-occurrence structure's `%ELEM` is its occurrences and
  `%SIZE(ds:*ALL)` their total.
- `INZ(*EXTDFT)` takes the file's `DFT` values; an `EXTNAME` structure
  names its external fields with `EXTFLD` (no parameter keeps the
  name) to change their `INZ`.

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
- `MOVE` into an indicator of a literal other than `'0'` or `'1'` is
  `RNF5184` (`'A'` too); a structure moved into its own subfield is
  `RNF7017`; `MOVEA` between numeric arrays of different digits or
  decimals is `RNF7262` (`5P 0` to `5S 0` is fine); `EVALR` into a
  varying field is `RNF7565`; `SUBST` with a constant length or start
  past the base is `RNF5215` / `RNF5216`; `%SUBST`'s are `RNF0365` /
  `RNF0364` (20).
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
- `COMMIT(*CHG)` starts commitment control at the first statement
  (`SQL050`-`SQL053`, the table journaled in `post:` steps): `ROLLBACK`
  undoes the updates, inserts and deletes since the last `COMMIT`. Over
  a table that is not journaled a change is `-7008 55019`, a query
  works and `COMMIT` and `ROLLBACK` are 0. A row an SQL statement
  changed stays locked to the commit against the program's own native
  open of the table (01299); a row a query or a cursor read does not,
  under `*CHG`, `*CS` or `*ALL` (`SQL052`-`SQL055`).
- A native read for update of a row with a null value under
  `ALWNULL(*NO)` is 01299 with `CPF5029` (the first versions of
  `SQL052`-`SQL054` read such rows); the file is noted `RNF7150` (00)
  when it is opened for update or output. QS/400 reads the row (null
  maps do not reach RPG, Blocked); the corpus reads rows without nulls.
- `PREPARE` reports `-104` and `-204`; `EXECUTE` of a statement whose
  `PREPARE` failed is `-518 07003`.
- `GET DIAGNOSTICS ... NUMBER` is 1 after a statement that worked.
- `SET OPTION` gives `SQL5066` (00) per option; a missing table is
  `SQL1103` (10) and `-204 42704` at run time; `DELETE` without `WHERE`
  is `SQL0088` (00).
- pub400's jobs run with CCSID 273: `CHAR` of a decimal has a comma.
  The corpus keeps to integers there.
- Cursors' lifetime (`SQL034`-`SQL038`: a `*NEW` program calls an SQL
  program twice). Under `CLOSQLCSR(*ENDACTGRP)` a cursor the called
  program left open in its caller's group is still open at the next
  call (`OPEN` `-502`, `FETCH` goes on to the next row), even after
  `LR`, and a statement it prepared is still prepared; in a `*NEW`
  group of its own the cursor is gone with the group. Under
  `CLOSQLCSR(*ENDMOD)`, from the command or `SET OPTION`, the return
  closes the cursor and discards the prepared statement, with `LR` or
  without (`EXECUTE` then `-518`).
- `SET OPTION CLOSQLCSR` in an ILE program takes `*ENDACTGRP` and
  `*ENDMOD`; `*ENDPGM`, `*ENDSQL`, `*ENDJOB` are `SQL0104` (30) at the
  word after the `*`, and `SQL9001` (`SQL039`-`SQL041`).
- `INCLUDE member` reads the member from the source file
  (`INCFILE(*SRCFILE)`), declarations or statements alike; the
  precompile listing lists its records in place, a `**FREE` source
  switching with `**END-FREE` and `**FREE` around a member that is not
  fully free, and the generated source has the statement as a
  `//*exec sql include` comment before them. A member not found is
  `SQL0004` (30), `SQL9001` (`SQL042`-`SQL044`).
- `DESCRIBE` (`SQL045`-`SQL047`): `SQLDAID` is `SQLDA`, `SQLDABC`
  `16 + 80 * SQLN`; each entry has the even type for `NOT NULL`
  columns and the odd one otherwise (`INTEGER` 496, `VARCHAR` 448,
  `CHAR` 452/453, `DECIMAL` 484/485 with `SQLLEN` precision * 256 +
  scale, `DATE` 384/385), `SQLNAMELEN` and `SQLNAME` the column's name
  or its `AS` name; `ID * 2` is `INTEGER`, `COUNT(*)` `INTEGER NOT
  NULL`. A `DATE` is 8 long under `DATFMT(*JOB)` on pub400 (a two-digit
  year job format). `DESCRIBE INPUT` gives the markers, null-capable,
  typed by the column they are compared with, named `00001`, `00002`, ... With `SQLN` too small
  `DESCRIBE` is SQLCODE 0 with `SQLD` set and no entries written; of a
  statement that is not a query `SQLD` 0; of a name never prepared
  `-516 26501`. `PREPARE ... INTO` describes as it prepares.
- `FETCH`, `EXECUTE` and `OPEN ... USING DESCRIPTOR` take the entries'
  `SQLDATA` (and for an odd type `SQLIND`) pointers, set with
  `%ADDR` in `SQLVAR` and copied into `SQL_VAR(i)` (`SQL048`,
  `SQL049`).

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
- **Options.** `*SRCSTMT` heads the rulers "Seq" and "Statement",
  prints sequence numbers where statement numbers were and in the
  cross-reference. `INDENT` gives fixed calculations their own ruler,
  the operation field indented by the given marks per level.
  `/TITLE` prints above every heading, `/SPACE` and `/EJECT` act and
  are neither printed nor numbered. `*NOEXPDDS` still numbers the
  generated lines but prints none of them; `*NOUNREF` hides nothing
  from the listing. Source binding directories print with a
  "Library" row under them.
- **After errors.** IBM goes on with what it can: a bad `LIKE` or
  `LIKEDS` makes the definition itself `RNF7030` too; decimals over the
  digits are kept to the digits; a bad length, dimension or prefix
  takes its limit, 1 or 2; a `LEN` stands though subfields pass it; a
  subfield named twice still takes its room; an unsigned `LIKE`
  adjustment is added; a basing pointer nothing declares is declared
  by `BASED`; a structure array without `QUALIFIED` keeps its
  subfields under it. A file use the device refuses (`RNF5276`,
  `RNF5202`) does not count as a use.
- **EVAL-CORR summary.** After the cross-reference, one summary per
  pair of structures: its statements, each target subfield, then
  `RNF7341` (target only) and `RNF7342` (source only), counted in the
  message summary. Each row's comment is a text (masked).

## Oracle facts: compile errors

Recorded from batch 12 (`b12err/`, ERR230-318), pub400 V7R5M0.

- Fixed forms come in order H, F and D, I, C, O in the main source; one
  out of order is `RNF0257` and ignored (a C after O specs leaves the
  program without a way to end, `RNF7023`).
- Fixed-form factors are names, not expressions: an undefined one is
  only `RNF7030`, never `RNF7503`; a factor 1 the operation ignores
  (`RNF5010`) is still resolved. A missing required factor 1 is
  `RNF5009`; digits run into a name (`9N98`) are `RNF0289`, the name
  kept.
- Entries IBM does not know take defaults at severity 20: file type
  (`RNF2003`, O or I), format (`RNF2006`, F), device (`RNF2013`, DISK),
  designation (`RNF2079`), definition type (`RNF3304`, S), data type
  (`RNF3302`, blank), conditioning (`RNF5007`) and output indicators
  (`RNF6003`), output type (`RNF6005`, D) and data format (`RNF6028`).
  An unknown file keyword is `RNF2023` (30).
- Directives: `/ENDIF` missing is `RNF0705` at the `/IF`; `/ELSE`,
  `/ELSEIF`, `/ENDIF` outside a group `RNF0706`; a bad condition
  `RNF0701`; `/DEFINE` without a name `RNF0702`; `/COPY` without a
  member `RNF0272`, one that cannot be opened `RNF0273` (40); an
  unknown directive `RNF0274`.
- The wrong `ENDyy` for the open group is `RNF5248` (20, END assumed);
  a `CASxx` group without `ENDCS` is `RNF5094` (20); a `GOTO`/`CABxx`
  target that is not a `TAG` is `RNF7030` and `RNF7043`; a `TAG`
  twice `RNF5013`.
- A prototype without `EXTPGM` under `DFTACTGRP(*YES)` is `RNF3788`; a
  main interface named other than the program is `RNF3776` (10).
- An indicator `*IN01`-`*IN99` nothing sets is `RNF7030` at its first
  use and `RNF7503` for each free-form statement using it
  (`ERR319`-`ERR340`): in an expression, as a call's argument (by
  reference or `CONST`), as a fixed-form factor, as a conditioning
  indicator of a calculation or an output record. It is set by
  `SETON`/`SETOFF` and any resulting indicator, an assignment
  (`*IN50 = `, `*IN(n) = ` with any index), `MOVEA` into `*IN(n)`, and by
  any use of the array itself (`*IN = *OFF`, `CLEAR *IN`, `%ADDR(*IN)`,
  even `IF *IN(50)`), and by a display file that has it as a response
  indicator, with `INDARA` or without, `EXFMT` or none. `PARM *IN50` is
  `RNF5179`.
- A child of `DCL-PR`, `DCL-PI` or `DCL-DS` named like a free-form
  operation code (`out`, `in`, `read`, `select`, `eval`, `clear`,
  `chain`, `return`, `for`, `dsply`, ...; not `add`, `move`, `time`) ends
  the group: `RNF3556` (20) at the group, the line read as a
  calculation (whatever that gives), the `END-PR` `RNF0724`
  (`ERR341`-`ERR370`). `DCL-PARM` and `DCL-SUBF` name them.

## Oracle facts: coverage of the operations, functions and keywords

Recorded from batch 15 (`b15cov/`, `gen/b15.py`), pub400 V7R5M0.

- A `CRTBNDRPG` without control options is `DFTACTGRP(*YES)`: every
  procedure is `RNF1520` (20) and `CALLB` `RNF5378` (30). A `USROPN`
  file no `OPEN` names is `RNF7062` (30). `(E)` on `ADD` or `Z-ADD` is
  `RNF5049` (20).
- `CASxx` takes no level in the listing's Do column; a `CASxx` naming
  no subroutine is `RNF7027`, a group with no `END` or `ENDCS`
  `RNF5094` (20), the group ending after it.
- `ALLOC` or `REALLOC` of length 0 is status 425, `REALLOC` of a pointer
  the heap did not give 426. `COMMIT` and `ROLBK` without commitment
  control are 802, and `RNF7099` (00) notes them when no file is under
  commitment control; with `STRCMTCTL ... CMTSCOPE(*JOB)` they work.
  `IN` of a data area that does not exist is 401; `OUT` of one the
  program has not locked is 412.
- A batch job's `DSPLY` with a response takes the default reply and
  leaves the field as it was. `DUMP` without `(A)` and without
  `DEBUG(*YES)` does nothing (`RNF5152`, 00); `DUMP(A)` always dumps,
  and its `QPPGMDMP` holds the job, the dates and the spooled file
  names, so no member prints one.
- `POST` of an open printer puts its name, status and `POST F` in the
  INFDS; of a closed one it is 01211. A `SHTDN` without a high
  indicator is `RNF5324` (30); `%SHTDN` is off.
- `WHEN-IS` of an operand that does not compare with `SELECT`'s is
  `RNF0203`, and so are `%PROC` with an operand, `%PASSED` of a name
  that is no parameter and `%OMITTED` of one without `*OMIT`;
  `%PARMNUM` of a name that is no parameter is `RNF0369` (20).
  `%PARMS` counts a `RTNPARM` procedure's hidden parameter.
- `%PROC()` is the module's name in the main procedure and the external
  name in a subprocedure (`EXTPROC(*DCLCASE)` keeps the source's case).
  `%HIVAL` and `%LOVAL` are in 7.5. `%THIS` outside a Java method is
  `RNF0380`; `%XML`, `%HANDLER`, `%DATA`, `%PARSER`, `%GEN` and
  `%TARGET` outside their operations are `RNF0637` (30), and what
  follows is read as an expression in parentheses.
- `%FLOAT` of bad character data is 105, `%SQRT` of a negative number
  101, `%SQRT('16')` `RNF0574` (20); `%UNS` and `%UNSH` of a negative
  number are 103, even one that rounds to zero. `%EDITFLT` gives
  `+1.500000000000000E+000` (23 characters) for an 8-byte float and
  `-1.2500000E-01` (14) for a 4-byte one. `%GRAPH` is `RNF0565` (30)
  while the CCSID of graphic data is ignored (the default), with
  `RNF0587` for a literal without shift characters and `RNF0556` for
  a number. `%NULLIND` of a field that is not null-capable is
  `RNF0342` (20). `%TLOOKUPLT` of a table without `ASCEND` is
  `RNF0507` (20).
- `SND-MSG *ESCAPE ... %TARGET(*SELF)` is caught with status 9999, an
  escape a called procedure sent to its caller with 202.
- `XML-INTO`: options in a literal are checked when compiling
  (`RNF0224`, 30); at run time a document that is not well formed is
  00351, a missing or extra element, a root of another name or a value
  that does not convert 00353. `trim=all` drops the ends and makes each
  inner run of blanks one. A `%HANDLER` procedure takes the
  communication area, a `CONST` array and `UNS(10) VALUE` (`RNF5352`
  otherwise). The PSDS (372-379) gets the number of elements set.
- `XML-SAX`: events without characters have length -1; an error in the
  handler ends the operation with that error's status (222 for a null
  pointer); a handler that returns nonzero ends it with 00351. pub400's
  CCSID 273 turns the CP037 `!` and `[` into other characters, so the
  corpus has no comment or CDATA in a document.
- `DATA-INTO` and `DATA-GEN` with a parser or generator that cannot be
  called are 00355.
- `SERIALIZE` needs `THREAD(*CONCURRENT)` (`RNF1519`, 20); a procedure's
  `PGMINFO` in a program is `RNF1524` (00). `SLN` needs a display
  record with `SLNO(*VAR)` (`RNF2029`). `PACKEVEN` on a subfield that is
  not packed is `RNF3704` (20). A `LIKEFILE` parent needs `BLOCK`
  (`RNF2590`).
- `EXTIND(*INU1)` with U1 off: the file is not opened, its writes are
  ignored, `%OPEN` is off. `FORMLEN` and `FORMOFL` set the page; the
  overflow indicator comes on once, when a line reaches the overflow
  line; pub400's footer takes a page's last line. `PRTCTL` gives space
  and skip as three digits each, and the line count after the line.
- A `SPECIAL` file whose program cannot be called opens with 01217.
  An Open Access `HANDLER` that cannot be called ends the program
  (`CEE9901`); `(E)` does not take it.
- Not verified by a created program: `%THIS` (only in a method of a
  Java class, `RNF0380` elsewhere) and `RAFDATA` (only for a record
  address file; `RNF2407` on an externally described one). `INFSR`
  belongs to the file exception work.
- The rpg-oracle tool now drops the spooled files of a member's setup
  and `dds:` compiles before its own: they name the library and user.

## Oracle facts: long varying fields, overlay arrays, files as parameters, null maps

Recorded from batch 16 (`b16ir/`), pub400 V7R5M0.

- A varying field longer than 65535 takes a 4-byte length (`%SIZE` of
  `varchar(70000)` is 70004); `varchar(10:4)`, `VARYING(4)` and
  `LEN(n) VARYING` in fixed form do so at any length. The string
  functions, concatenation and parameters (by reference, `CONST`,
  `VALUE`, the return value) work at those lengths; a value past the
  maximum is cut to it.
- `*BLANKS` into a varying field fills it to its maximum length, not
  its current one.
- `FOR i = 1 TO n / 10` with an integer `i` is `RNF7569` (20).
- A subfield overlaying an array subfield is an array with the base's
  element spacing (`%ELEM` the base's, `%SIZE(x:*ALL)` its own size
  times the elements); `SORTA` of it moves the base's whole elements,
  `%LOOKUP` and `%XFOOT` see only it.
- Through a `LIKEFILE` parameter the procedure reads the caller's open
  file: the caller's position moves, and `%EOF` the procedure leaves
  is the caller's. Each parameter file's format is `RNF7066` when no
  operation names it.
- Under `ALWNULL(*USRCTL)` a null character field reads as blanks;
  `UPDATE ... %FIELDS(a:b)` stores the null indicators of the named
  fields, `WRITE` those of every field.
- `VARGRAPH` data moves without conversion under the job's CCSID 273;
  `%SUBST` of graphic data is graphic.

## Feature matrix

<!-- matrix: generated by tools/conformance/rpg/gen/matrix.py -->

### Operation codes

| Operation | Status | Members |
|---|---|---|
| `ACQ` | verified, V7R5M0 | COV126 |
| `ADD` | verified, V7R5M0 | SPEC002, SPEC006, SPEC011, CYC002 and 11 more |
| `ADDDUR` | verified, V7R5M0 | DTE023 |
| `ALLOC` | verified, V7R5M0 | COV005, COV006 |
| `ANDxx` | verified, V7R5M0 | SPEC006, COV029 |
| `BEGSR` | verified, V7R5M0 | ARR009, CYC013, DSP013, EXC003 and 15 more |
| `BITOFF` | verified, V7R5M0 | BIT001 |
| `BITON` | verified, V7R5M0 | BIT001 |
| `CABxx` | verified, V7R5M0 | SPEC006 |
| `CALL` | verified, V7R5M0 | PRC009, LST036 |
| `CALLB` | verified, V7R5M0 | COV130 |
| `CALLP` | verified, V7R5M0 | EXC010, COV053, COV055, COV067 and 3 more |
| `CASxx` | verified, V7R5M0 | COV009 |
| `CAT` | verified, V7R5M0 | STR001 |
| `CHAIN` | verified, V7R5M0 | IO002, IO003, IO007, IO008 and 24 more |
| `CHECK` | verified, V7R5M0 | STR003 |
| `CHECKR` | verified, V7R5M0 | STR003 |
| `CLEAR` | verified, V7R5M0 | ARR009, DTE001, OPS004, COV023 |
| `CLOSE` | verified, V7R5M0 | IO012, IO013, IO030, DSP016 and 2 more |
| `COMMIT` | verified, V7R5M0 | COV012, COV013 |
| `COMP` | verified, V7R5M0 | BIT003, LST024 |
| `DATA-GEN` | verified, V7R5M0 | COV014 |
| `DATA-INTO` | verified, V7R5M0 | COV015, COV016 |
| `DEALLOC` | verified, V7R5M0 | PRC012, COV005, COV074 |
| `DEFINE` | verified, V7R5M0 | COV017 |
| `DELETE` | verified, V7R5M0 | IO008, IO024 |
| `DIV` | verified, V7R5M0 | SPEC002, BIT004 |
| `DO` | verified, V7R5M0 | LST016, COV009, COV029, COV115 |
| `DOU` | verified, V7R5M0 | OPS001, DSP001, DSP002, DSP003 and 17 more |
| `DOUxx` | verified, V7R5M0 | SPEC006, DSP017 |
| `DOW` | verified, V7R5M0 | OPS001, IO001, IO003, IO004 and 26 more |
| `DOWxx` | verified, V7R5M0 | SPEC006, SPEC011, IO015, IO016 and 3 more |
| `DSPLY` | verified, V7R5M0 | COV019, COV121 |
| `DUMP` | verified, V7R5M0 | COV020 |
| `ELSE` | verified, V7R5M0 | SPEC006, EXP126, EXP127, BIT007 and 138 more |
| `ELSEIF` | verified, V7R5M0 | DSP002, FUZ0000, FUZ0001, FUZ0002 and 120 more |
| `ENDCS` | verified, V7R5M0 | COV009 |
| `ENDDO` | verified, V7R5M0 | SPEC006, SPEC011, OPS001, IO001 and 55 more |
| `ENDFOR` | verified, V7R5M0 | DCL016, DCL022, ARR007, ARR010 and 166 more |
| `ENDIF` | verified, V7R5M0 | SPEC006, SPEC008, EXP126, EXP127 and 198 more |
| `ENDMON` | verified, V7R5M0 | EXP001, EXP002, EXP003, EXP004 and 280 more |
| `ENDSL` | verified, V7R5M0 | DTE014, OPS002, DSP011, ERR184 and 132 more |
| `ENDSR` | verified, V7R5M0 | ARR009, CYC013, DSP013, EXC003 and 15 more |
| `EVAL` | verified, V7R5M0 | SPEC001, SPEC002, SPEC003, SPEC004 and 331 more |
| `EVALR` | verified, V7R5M0 | OPS006, STR006 |
| `EVAL-CORR` | verified, V7R5M0 | BIT006 |
| `EXCEPT` | verified, V7R5M0 | SPEC017, IO016, LST031, COV097 |
| `EXFMT` | verified, V7R5M0 | DSP001, DSP002, DSP003, DSP004 and 18 more |
| `EXSR` | verified, V7R5M0 | DSP013, EXC004, FEX014, LST024 and 2 more |
| `EXTRCT` | verified, V7R5M0 | DTE023 |
| `FEOD` | verified, V7R5M0 | IO020 |
| `FOR` | verified, V7R5M0 | DCL016, DCL022, ARR007, ARR010 and 165 more |
| `FOR-EACH` | verified, V7R5M0 | STR010 |
| `FORCE` | verified, V7R5M0 | COV021 |
| `GOTO` | verified, V7R5M0 | SPEC006, LST024 |
| `IF` | verified, V7R5M0 | SPEC008, EXP126, EXP127, ARR006 and 193 more |
| `IFxx` | verified, V7R5M0 | SPEC006, CYC002, CYC006, LST016 and 1 more |
| `IN` | verified, V7R5M0 | COV022, COV023, COV024, COV120 |
| `ITER` | verified, V7R5M0 | OPS001 |
| `KFLD` | verified, V7R5M0 | IO017, LST036 |
| `KLIST` | verified, V7R5M0 | IO017, LST036 |
| `LEAVE` | verified, V7R5M0 | OPS001, DSP005, DSP011, DSP012 and 9 more |
| `LEAVESR` | verified, V7R5M0 | COV025 |
| `LOOKUP` | verified, V7R5M0 | ARR001, ARR002, ARR003 |
| `MHHZO` | verified, V7R5M0 | MOV020 |
| `MHLZO` | verified, V7R5M0 | MOV020 |
| `MLHZO` | verified, V7R5M0 | MOV020 |
| `MLLZO` | verified, V7R5M0 | MOV020 |
| `MONITOR` | verified, V7R5M0 | EXP001, EXP002, EXP003, EXP004 and 280 more |
| `MOVE` | verified, V7R5M0 | MOV001, MOV003, MOV005, MOV006 and 11 more |
| `MOVEA` | verified, V7R5M0 | MOV016, MOV017, MOV018, ERR305 |
| `MOVEL` | verified, V7R5M0 | MOV002, MOV004, MOV005, MOV007 and 10 more |
| `MULT` | verified, V7R5M0 | SPEC002, CYC012 |
| `MVR` | verified, V7R5M0 | BIT004 |
| `NEXT` | verified, V7R5M0 | COV126 |
| `OCCUR` | verified, V7R5M0 | ARR008, ARR009 |
| `ON-ERROR` | verified, V7R5M0 | EXP001, EXP002, EXP003, EXP004 and 278 more |
| `ON-EXCP` | verified, V7R5M0 | EXC011, COV055 |
| `ON-EXIT` | verified, V7R5M0 | EXC010 |
| `OPEN` | verified, V7R5M0 | IO012, IO013, IO030, DSP016 and 11 more |
| `ORxx` | verified, V7R5M0 | SPEC006, COV029 |
| `OTHER` | verified, V7R5M0 | DTE014, OPS002, FUZ0001, FUZ0002 and 120 more |
| `OUT` | verified, V7R5M0 | COV022, COV023, COV024 |
| `PARM` | verified, V7R5M0 | PRC009, LST036, COV130 |
| `PLIST` | verified, V7R5M0 | PRC009, LST036 |
| `POST` | verified, V7R5M0 | COV004 |
| `READ` | verified, V7R5M0 | IO001, IO003, IO004, IO006 and 31 more |
| `READC` | verified, V7R5M0 | DSP011, DSP012 |
| `READE` | verified, V7R5M0 | IO003, IO009, IO017, IO023 and 2 more |
| `READP` | verified, V7R5M0 | IO004, IO018 |
| `READPE` | verified, V7R5M0 | IO028 |
| `REALLOC` | verified, V7R5M0 | COV005, COV006 |
| `REL` | verified, V7R5M0 | COV126 |
| `RESET` | verified, V7R5M0 | ARR009 |
| `RETURN` | verified, V7R5M0 | SPEC016, DCL022, DTE019, IO030 and 142 more |
| `ROLBK` | verified, V7R5M0 | COV012, COV013 |
| `SCAN` | verified, V7R5M0 | STR002 |
| `SELECT` | verified, V7R5M0 | DTE014, OPS002, DSP011, ERR184 and 132 more |
| `SETGT` | verified, V7R5M0 | IO004, IO005, IO014, IO018 and 1 more |
| `SETLL` | verified, V7R5M0 | IO003, IO004, IO005, IO006 and 13 more |
| `SETOFF` | verified, V7R5M0 | SPEC005, MOV018 |
| `SETON` | verified, V7R5M0 | SPEC005, DTE023, DTE024, DTE025 and 22 more |
| `SHTDN` | verified, V7R5M0 | COV027 |
| `SORTA` | verified, V7R5M0 | ARR005, ARR010, IR006 |
| `SQRT` | verified, V7R5M0 | BIT004 |
| `SUB` | verified, V7R5M0 | SPEC002 |
| `SUBDUR` | verified, V7R5M0 | DTE023 |
| `SUBST` | verified, V7R5M0 | STR005 |
| `TAG` | verified, V7R5M0 | SPEC006, SQL017, LST024, IR008 |
| `TEST` | verified, V7R5M0 | DTE024 |
| `TESTB` | verified, V7R5M0 | BIT001 |
| `TESTN` | verified, V7R5M0 | BIT002 |
| `TESTZ` | verified, V7R5M0 | BIT002 |
| `TIME` | verified, V7R5M0 | DTE025 |
| `UNLOCK` | verified, V7R5M0 | IO019, COV120 |
| `UPDATE` | verified, V7R5M0 | IO007, IO013, IO018, IO019 and 14 more |
| `WHEN` | verified, V7R5M0 | DTE014, OPS002, DSP011, FUZ0001 and 129 more |
| `WHENxx` | verified, V7R5M0 | COV029 |
| `WHEN-IN` | verified, V7R5M0 | COV031 |
| `WHEN-IS` | verified, V7R5M0 | COV031 |
| `WRITE` | verified, V7R5M0 | SPEC001, SPEC002, SPEC003, SPEC004 and 621 more |
| `XFOOT` | verified, V7R5M0 | ARR004 |
| `XLATE` | verified, V7R5M0 | STR004 |
| `XML-INTO` | verified, V7R5M0 | COV033, COV034, COV035, COV037 and 2 more |
| `XML-SAX` | verified, V7R5M0 | COV038, COV039, COV040, COV041 |
| `Z-ADD` | verified, V7R5M0 | SPEC002, SPEC006, SPEC011, CYC002 and 7 more |
| `Z-SUB` | verified, V7R5M0 | SPEC002 |

### Built-in functions

| Function | Status | Members |
|---|---|---|
| `%ABS` | verified, V7R5M0 | EXP127, BIF005, DTE022, DTE025 and 132 more |
| `%ADDR` | verified, V7R5M0 | DCL020, STR008, PRC002, SQL048 and 2 more |
| `%ALLOC` | verified, V7R5M0 | PRC012, COV074 |
| `%BITAND` | verified, V7R5M0 | BIT005 |
| `%BITNOT` | verified, V7R5M0 | BIT005 |
| `%BITOR` | verified, V7R5M0 | BIT005 |
| `%BITXOR` | verified, V7R5M0 | BIT005 |
| `%CHAR` | verified, V7R5M0 | SPEC001, SPEC002, SPEC003, SPEC004 and 555 more |
| `%CHARCOUNT` | verified, V7R5M0 | STR008 |
| `%CHECK` | verified, V7R5M0 | BIF003 |
| `%CHECKR` | verified, V7R5M0 | BIF003, IR001 |
| `%CONCAT` | verified, V7R5M0 | STR009 |
| `%CONCATARR` | verified, V7R5M0 | STR009 |
| `%DATA` | verified, V7R5M0 | COV014, COV015, COV016 |
| `%DATE` | verified, V7R5M0 | DTE007, DTE015, DTE017, DTE022 and 4 more |
| `%DAYS` | verified, V7R5M0 | DTE009, DTE012, DTE013, DTE017 and 3 more |
| `%DEC` | verified, V7R5M0 | BIF005, BIT004, DTE006, DTE028 and 131 more |
| `%DECH` | verified, V7R5M0 | BIF005, COV042 |
| `%DECPOS` | verified, V7R5M0 | DCL001, DCL002, DCL003, DCL004 and 10 more |
| `%DIFF` | verified, V7R5M0 | DTE010, DTE011, DTE012, DTE017 and 4 more |
| `%DIV` | verified, V7R5M0 | BIF008, DTE025, IR004 |
| `%EDITC` | verified, V7R5M0 | BIF006, MOV006, MOV007, MOV008 and 21 more |
| `%EDITFLT` | verified, V7R5M0 | COV045 |
| `%EDITW` | verified, V7R5M0 | BIF007 |
| `%ELEM` | verified, V7R5M0 | SPEC004, DCL016, DCL018, DCL022 and 3 more |
| `%EOF` | verified, V7R5M0 | IO001, IO002, IO003, IO004 and 20 more |
| `%EQUAL` | verified, V7R5M0 | IO005 |
| `%ERROR` | verified, V7R5M0 | IO013, IO020, DSP016, FEX003 and 9 more |
| `%FIELDS` | verified, V7R5M0 | IO007, IR008 |
| `%FLOAT` | verified, V7R5M0 | COV042, COV043 |
| `%FOUND` | verified, V7R5M0 | IO002, IO003, IO005, IO008 and 10 more |
| `%GEN` | verified, V7R5M0 | COV014 |
| `%GRAPH` | verified, V7R5M0 | COV131 |
| `%HANDLER` | verified, V7R5M0 | COV016, COV037, COV038, COV039 and 2 more |
| `%HIVAL` | verified, V7R5M0 | COV065, COV066 |
| `%HOURS` | verified, V7R5M0 | DTE011, DTE012, DTE017, DTE018 |
| `%INT` | verified, V7R5M0 | EXP127, BIF005, PRC001, FUZ0000 and 136 more |
| `%INTH` | verified, V7R5M0 | BIF005 |
| `%KDS` | verified, V7R5M0 | IO003, IO011, LST014, LST015 |
| `%LEFT` | verified, V7R5M0 | STR008 |
| `%LEN` | verified, V7R5M0 | DCL001, DCL002, DCL003, DCL004 and 159 more |
| `%LIST` | verified, V7R5M0 | BIT007, COV031 |
| `%LOOKUP` | verified, V7R5M0 | DCL016, ARR006, ARR010, IR006 |
| `%LOOKUPLT` | verified, V7R5M0 | ARR006 |
| `%LOOKUPLE` | verified, V7R5M0 | ARR006 |
| `%LOOKUPGT` | verified, V7R5M0 | ARR006 |
| `%LOOKUPGE` | verified, V7R5M0 | ARR006, ERR315 |
| `%LOVAL` | verified, V7R5M0 | COV065 |
| `%LOWER` | verified, V7R5M0 | BIF004 |
| `%MAX` | verified, V7R5M0 | BIF009, DTE014 |
| `%MAXARR` | verified, V7R5M0 | ARR007 |
| `%MIN` | verified, V7R5M0 | BIF009, DTE014 |
| `%MINARR` | verified, V7R5M0 | ARR007 |
| `%MINUTES` | verified, V7R5M0 | DTE011, DTE012, DTE017, DTE019 |
| `%MONTHS` | verified, V7R5M0 | DTE009, DTE012, DTE018 |
| `%MSECONDS` | verified, V7R5M0 | DTE012, DTE017, DTE021 |
| `%NULLIND` | verified, V7R5M0 | COV051, COV118, IR008 |
| `%OCCUR` | verified, V7R5M0 | ARR008 |
| `%OMITTED` | verified, V7R5M0 | COV067 |
| `%OPEN` | verified, V7R5M0 | IO012, IO030, DSP016, FEX011 and 1 more |
| `%PADDR` | verified, V7R5M0 | PRC007, PRC008, COV130 |
| `%PARMNUM` | verified, V7R5M0 | COV070 |
| `%PARMS` | verified, V7R5M0 | PRC002, COV067, COV070, COV090 and 1 more |
| `%PARSER` | verified, V7R5M0 | COV015, COV016 |
| `%PASSED` | verified, V7R5M0 | COV067 |
| `%PROC` | verified, V7R5M0 | COV053 |
| `%RANGE` | verified, V7R5M0 | BIT007, COV031 |
| `%REALLOC` | verified, V7R5M0 | PRC012 |
| `%REM` | verified, V7R5M0 | EXP127, BIF008, DSP003, DSP008 and 129 more |
| `%REPLACE` | verified, V7R5M0 | BIF004, IR001 |
| `%RIGHT` | verified, V7R5M0 | STR008 |
| `%SCAN` | verified, V7R5M0 | BIF002, UCS001, FUZ0000, FUZ0001 and 117 more |
| `%SCANR` | verified, V7R5M0 | STR007 |
| `%SCANRPL` | verified, V7R5M0 | STR007 |
| `%SECONDS` | verified, V7R5M0 | DTE011, DTE012, DTE017 |
| `%SHTDN` | verified, V7R5M0 | COV027 |
| `%SIZE` | verified, V7R5M0 | DCL001, DCL002, DCL003, DCL004 and 32 more |
| `%SPLIT` | verified, V7R5M0 | STR010 |
| `%SQRT` | verified, V7R5M0 | COV042, COV043 |
| `%STATUS` | verified, V7R5M0 | EXP126, EXP127, IO013, IO018 and 181 more |
| `%STR` | verified, V7R5M0 | STR008 |
| `%SUBARR` | verified, V7R5M0 | ARR005, ARR007 |
| `%SUBDT` | verified, V7R5M0 | DTE011, DTE012, DTE013, DTE017 and 1 more |
| `%SUBST` | verified, V7R5M0 | BIF002, STR008, UCS001, IO011 and 131 more |
| `%TARGET` | verified, V7R5M0 | COV055 |
| `%THIS` | compiled, not in the corpus | |
| `%TIME` | verified, V7R5M0 | DTE008, DTE015, DTE022, DTE025 and 2 more |
| `%TIMESTAMP` | verified, V7R5M0 | DTE008, DTE017, DTE022, DTE025 and 1 more |
| `%TLOOKUP` | verified, V7R5M0 | ARR006 |
| `%TLOOKUPLT` | verified, V7R5M0 | COV058 |
| `%TLOOKUPLE` | verified, V7R5M0 | ARR006 |
| `%TLOOKUPGT` | verified, V7R5M0 | ARR006 |
| `%TLOOKUPGE` | verified, V7R5M0 | COV058 |
| `%TRIM` | verified, V7R5M0 | SPEC008, BIF001, OPS006, STR006 and 133 more |
| `%TRIML` | verified, V7R5M0 | BIF001, FUZ0001, FUZ0002, FUZ0008 and 111 more |
| `%TRIMR` | verified, V7R5M0 | BIF001, STR008, FUZ0001, FUZ0002 and 120 more |
| `%UCS2` | verified, V7R5M0 | UCS001, IR002, IR003 |
| `%UNS` | verified, V7R5M0 | COV047, COV048 |
| `%UNSH` | verified, V7R5M0 | COV047, COV048 |
| `%UPPER` | verified, V7R5M0 | BIF004 |
| `%XFOOT` | verified, V7R5M0 | ARR004, ARR007, ARR010, BIF008 and 1 more |
| `%XLATE` | verified, V7R5M0 | BIF003, ERR167 |
| `%XML` | verified, V7R5M0 | COV033, COV034, COV035, COV037 and 6 more |
| `%YEARS` | verified, V7R5M0 | DTE009, DTE012, DTE017, DTE018 and 1 more |

### Control keywords

| Keyword | Status | Members |
|---|---|---|
| `ACTGRP` | verified, V7R5M0 | SQL034, SQL035, SQL036, SQL037 and 3 more |
| `ALLOC` | verified, V7R5M0 | COV005, COV006, COV074 |
| `ALTSEQ` | verified, V7R5M0 | COV091, COV092 |
| `ALWNULL` | verified, V7R5M0 | COV051, COV118, COV119, IR008 |
| `AUT` | verified, V7R5M0 | COV073 |
| `BNDDIR` | verified, V7R5M0 | COV094 |
| `CCSID` | verified, V7R5M0 | COV089, COV131 |
| `CCSIDCVT` | verified, V7R5M0 | COV088 |
| `COPYNEST` | verified, V7R5M0 | COV083 |
| `COPYRIGHT` | verified, V7R5M0 | COV072 |
| `CURSYM` | verified, V7R5M0 | COV076 |
| `CVTOPT` | verified, V7R5M0 | COV088 |
| `DATEDIT` | verified, V7R5M0 | COV075 |
| `DATFMT` | verified, V7R5M0 | DCL012, DCL013, DCL014, DTE023 and 5 more |
| `DCLOPT` | verified, V7R5M0 | COV074 |
| `DEBUG` | verified, V7R5M0 | COV020 |
| `DECEDIT` | verified, V7R5M0 | COV076 |
| `DECPREC` | verified, V7R5M0 | COV077 |
| `DFTACTGRP` | verified, V7R5M0 | DCL001, DCL002, DCL003, DCL004 and 555 more |
| `DFTNAME` | verified, V7R5M0 | COV072 |
| `ENBPFRCOL` | verified, V7R5M0 | COV072 |
| `EXPROPTS` | verified, V7R5M0 | EXP003, EXP010, EXP017, EXP024 and 13 more |
| `EXTBININT` | verified, V7R5M0 | COV078 |
| `FIXNBR` | verified, V7R5M0 | COV079 |
| `FLTDIV` | verified, V7R5M0 | COV080 |
| `FORMSALIGN` | verified, V7R5M0 | COV091 |
| `FTRANS` | verified, V7R5M0 | COV091 |
| `GENLVL` | verified, V7R5M0 | COV081 |
| `INDENT` | verified, V7R5M0 | COV083 |
| `INTPREC` | verified, V7R5M0 | COV081, COV082 |
| `LANGID` | verified, V7R5M0 | COV073, COV093 |
| `MAIN` | verified, V7R5M0 | EXC002, COV053 |
| `NOMAIN` | verified, V7R5M0 | SQL028, LST026 |
| `OPENOPT` | verified, V7R5M0 | COV132 |
| `OPTIMIZE` | verified, V7R5M0 | COV072 |
| `OPTION` | verified, V7R5M0 | SQL018 |
| `PGMINFO` | verified, V7R5M0 | COV090 |
| `PRFDTA` | verified, V7R5M0 | COV072 |
| `REQPREXP` | verified, V7R5M0 | COV132 |
| `SRTSEQ` | verified, V7R5M0 | COV083, COV093 |
| `STGMDL` | verified, V7R5M0 | COV073 |
| `TEXT` | verified, V7R5M0 | DTE027, COV034, COV073 |
| `THREAD` | verified, V7R5M0 | COV086, COV111 |
| `TIMFMT` | verified, V7R5M0 | DCL010, DCL011, DCL012, DCL013 and 3 more |
| `TRUNCNBR` | verified, V7R5M0 | COV127, COV128 |
| `USRPRF` | verified, V7R5M0 | COV073 |
| `VALIDATE` | verified, V7R5M0 | COV088 |

### File keywords

| Keyword | Status | Members |
|---|---|---|
| `ALIAS` | verified, V7R5M0 | COV104, COV105 |
| `BLOCK` | verified, V7R5M0 | IO016, COV108, COV129, IR007 |
| `COMMIT` | verified, V7R5M0 | SQL018, SQL019, COV012, COV013 |
| `DATFMT` | verified, V7R5M0 | DCL012, DCL013, DCL014, DTE023 and 5 more |
| `DEVID` | verified, V7R5M0 | COV126 |
| `EXTDESC` | verified, V7R5M0 | IO016, FEX011 |
| `EXTFILE` | verified, V7R5M0 | IO016, FEX011, COV108 |
| `EXTIND` | verified, V7R5M0 | COV095 |
| `EXTMBR` | verified, V7R5M0 | COV106 |
| `FORMLEN` | verified, V7R5M0 | COV096 |
| `FORMOFL` | verified, V7R5M0 | COV096 |
| `HANDLER` | verified, V7R5M0 | COV102 |
| `IGNORE` | verified, V7R5M0 | COV133 |
| `INCLUDE` | verified, V7R5M0 | SQL042, SQL043, SQL045, SQL046 and 5 more |
| `INDDS` | verified, V7R5M0 | DSP008 |
| `INFDS` | verified, V7R5M0 | IO014, IO022, IO024, DSP007 and 9 more |
| `INFSR` | verified, V7R5M0 | FEX003, FEX004, FEX005, FEX006 and 3 more |
| `KEYED` | verified, V7R5M0 | IO001, IO002, IO003, IO004 and 40 more |
| `KEYLOC` | verified, V7R5M0 | COV098 |
| `LIKEFILE` | verified, V7R5M0 | COV108, COV129, IR007 |
| `MAXDEV` | verified, V7R5M0 | COV126, COV134 |
| `OFLIND` | verified, V7R5M0 | SPEC017, CYC001, CYC002, CYC003 and 13 more |
| `PASS` | verified, V7R5M0 | DSP003 |
| `PGMNAME` | verified, V7R5M0 | COV099, COV100 |
| `PLIST` | verified, V7R5M0 | PRC009, LST036 |
| `PREFIX` | verified, V7R5M0 | IO009, IO011, IO016 |
| `PRTCTL` | verified, V7R5M0 | COV097 |
| `QUALIFIED` | verified, V7R5M0 | DCL017, DCL018, DCL019, DCL020 and 36 more |
| `RAFDATA` | compiled, not in the corpus | |
| `RECNO` | verified, V7R5M0 | COV106 |
| `RENAME` | verified, V7R5M0 | IO009, IO016, IR008 |
| `SAVEDS` | verified, V7R5M0 | COV134 |
| `SAVEIND` | verified, V7R5M0 | COV134 |
| `SFILE` | verified, V7R5M0 | DSP010, DSP011, DSP012, DSP013 and 3 more |
| `SLN` | compiled, not in the corpus | |
| `STATIC` | verified, V7R5M0 | DCL022, PRC008 |
| `TEMPLATE` | verified, V7R5M0 | DCL018, COV016, COV037 |
| `TIMFMT` | verified, V7R5M0 | DCL010, DCL011, DCL012, DCL013 and 3 more |
| `USAGE` | verified, V7R5M0 | IO006, IO007, IO008, IO013 and 28 more |
| `USROPN` | verified, V7R5M0 | IO012, IO013, IO030, DSP016 and 11 more |

### Definition keywords

| Keyword | Status | Members |
|---|---|---|
| `ALIAS` | verified, V7R5M0 | COV104, COV105 |
| `ALIGN` | verified, V7R5M0 | COV109 |
| `ALT` | verified, V7R5M0 | ARR003, ARR006, LST019, COV058 |
| `ALTSEQ` | verified, V7R5M0 | COV091, COV092 |
| `ASCEND` | verified, V7R5M0 | DCL016, ARR002, ARR006, ERR315 and 1 more |
| `BASED` | verified, V7R5M0 | DCL020, PRC012, ERR112, COV005 and 4 more |
| `CCSID` | verified, V7R5M0 | COV089, COV131 |
| `CLASS` | verified, V7R5M0 | COV111 |
| `CONST` | verified, V7R5M0 | DCL015, DTE019, PRC001, PRC002 and 10 more |
| `CTDATA` | verified, V7R5M0 | SPEC011, ARR003, ARR006, LST019 and 3 more |
| `DATFMT` | verified, V7R5M0 | DCL012, DCL013, DCL014, DTE023 and 5 more |
| `DESCEND` | verified, V7R5M0 | ARR002, ARR005 |
| `DIM` | verified, V7R5M0 | SPEC004, SPEC011, DCL016, DCL018 and 36 more |
| `DTAARA` | verified, V7R5M0 | COV022, COV023, COV024, COV117 and 1 more |
| `EXPORT` | verified, V7R5M0 | SQL028, LST026, COV130 |
| `EXT` | verified, V7R5M0 | CYC012 |
| `EXTFLD` | compiled, not in the corpus | |
| `EXTFMT` | compiled, not in the corpus | |
| `EXTNAME` | verified, V7R5M0 | IO011, LST014, LST015, COV078 and 1 more |
| `EXTPGM` | verified, V7R5M0 | EXC012, PRC006, PRC010, PRC011 and 5 more |
| `EXTPROC` | verified, V7R5M0 | PRC007, PRC008, LST009, LST010 and 5 more |
| `FROMFILE` | verified, V7R5M0 | COV115 |
| `IMPORT` | verified, V7R5M0 | PRC005, LST026 |
| `INZ` | verified, V7R5M0 | SPEC001, SPEC002, SPEC004, SPEC006 and 419 more |
| `LEN` | verified, V7R5M0 | DCL001, DCL002, DCL003, DCL004 and 521 more |
| `LIKE` | verified, V7R5M0 | DCL015, DCL016, SQL026 |
| `LIKEDS` | verified, V7R5M0 | DCL018, COV016, COV037, IR004 |
| `LIKEFILE` | verified, V7R5M0 | COV108, COV129, IR007 |
| `LIKEREC` | verified, V7R5M0 | IO003, IO010, IO021, COV104 and 3 more |
| `NOOPT` | verified, V7R5M0 | COV116 |
| `NULLIND` | verified, V7R5M0 | COV051 |
| `OCCURS` | verified, V7R5M0 | ARR008, ARR009 |
| `OPDESC` | verified, V7R5M0 | COV116 |
| `OPTIONS` | verified, V7R5M0 | PRC002, PRC010, PRC011, COV067 and 2 more |
| `OVERLAY` | verified, V7R5M0 | DCL019, BIT005, DTE024, COV079 and 1 more |
| `PACKEVEN` | verified, V7R5M0 | COV109 |
| `PERRCD` | verified, V7R5M0 | SPEC011, ARR003, ARR006, LST019 and 4 more |
| `POS` | verified, V7R5M0 | DCL017, STR002, STR003, IO014 and 14 more |
| `PREFIX` | verified, V7R5M0 | IO009, IO011, IO016 |
| `PSDS` | verified, V7R5M0 | EXC003, EXC004, EXC005, EXC009 and 5 more |
| `QUALIFIED` | verified, V7R5M0 | DCL017, DCL018, DCL019, DCL020 and 36 more |
| `RTNPARM` | verified, V7R5M0 | COV090, COV116 |
| `SAMEPOS` | verified, V7R5M0 | COV109 |
| `STATIC` | verified, V7R5M0 | DCL022, PRC008 |
| `TEMPLATE` | verified, V7R5M0 | DCL018, COV016, COV037 |
| `TIMFMT` | verified, V7R5M0 | DCL010, DCL011, DCL012, DCL013 and 3 more |
| `TOFILE` | verified, V7R5M0 | COV114 |
| `VALUE` | verified, V7R5M0 | SPEC016, DTE019, EXC006, FEX009 and 132 more |
| `VARYING` | verified, V7R5M0 | SPEC001, SPEC003, SPEC004, DCL010 and 11 more |

### Procedure keywords

| Keyword | Status | Members |
|---|---|---|
| `EXPORT` | verified, V7R5M0 | SQL028, LST026, COV130 |
| `PGMINFO` | verified, V7R5M0 | COV090 |
| `REQPROTO` | verified, V7R5M0 | COV090 |
| `RTNPARM` | verified, V7R5M0 | COV090, COV116 |
| `SERIALIZE` | verified, V7R5M0 | COV086 |
| `STATIC` | verified, V7R5M0 | DCL022, PRC008 |

<!-- end matrix -->
