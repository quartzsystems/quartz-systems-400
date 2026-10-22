# The command analyzer's oracle kit

`strings.txt` holds command strings, one per line, chosen to exercise
the analyzer: positional and keyword forms, every scalar type, lists,
groups and qualified names, special values, bad keywords, bad
restricted values, missing required parameters, selective prompting
prefixes, labels and library-qualified command names, and the CL
statements the compiler analyzes as commands.

`expected.txt` is what this system's analyzer makes of each string:
`OK` and the command as the analyzer spells it, or the escape message
identifier followed by the diagnostics. `cargo test -p conformance
--test cmd_kit` compares a run with it; `QS400_UPDATE_CMD=1` rewrites
it after a deliberate change.

## The real box

`ORACLE.CLLE` runs the same strings through `QCMDCHK` on an IBM i and
sends one message per string to a message queue. To gather a
comparison file:

1. `CRTSRCPF FILE(ORACLE/QCMDSRC) RCDLEN(524)` and copy `strings.txt`
   into member `STRINGS`, one string per record.
2. Compile `ORACLE.CLLE` into library `ORACLE` with `CRTBNDCL`.
3. `CALL ORACLE/ORACLE PARM(ORACLE)`.
4. `DSPMSG MSGQ(ORACLE/ORACLE) OUTPUT(*PRINT)` and keep the spooled
   file as `oracle.txt` beside this file.

Each message reads `n OK command` or `n CPF0006 CPDxxxx` (`QCMDCHK`'s
escape and the first diagnostic); `n` is the line number in
`strings.txt`. `cargo test -p conformance --test cmd_oracle` reads
`oracle.txt` beside this file, compares the box's first diagnostic with
the identifier this analyzer sends for the same string and the accepted
strings with ours, and reports itself skipped while the file is absent.
`QCMDCHK` echoes the string as typed, so spellings are not compared. No
such capture is checked in yet (`docs/CMD_OPEN_QUESTIONS.md`).
