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

## Blocked

- **The primary oracle has no RPG compiler.** Every compile-and-run
  result is captured on pub400 (V7R5M0) instead; nothing in the corpus
  is verified on V7R1M0.
- **HLINV is on neither oracle.** No user library on Cody's box holds
  a source file, an RPG object or anything named `HLINV`, and pub400's
  account has none either. The HLINV harness skips with a message
  until its source is placed in `tools/conformance/hlinv/src/`.
