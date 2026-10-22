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

Each message reads `n OK command` or `n CPDxxxx`; `n` is the line
number in `strings.txt`. The real box reports only the escape, while
`expected.txt` also lists the diagnostics, so a comparison looks at
the first identifier of each outcome and at the spelling of the
accepted commands. No such capture is checked in yet
(`docs/CMD_OPEN_QUESTIONS.md`).
