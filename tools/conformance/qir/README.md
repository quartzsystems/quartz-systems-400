# IR conformance kit

Sixty-odd `.qir` programs under `programs/`, one case per file, that
the in-memory runtime executes and whose outcomes live under
`expected/`: the console lines, the messages raised and handled, what
was left pending, the inquiry replies given and whether a dump was
written. `cargo test -p conformance --test qir_kit` runs every case,
writes the outcomes under `target/conformance-qir/ours/` and compares
them file by file; `QS400_UPDATE_QIR=1` rewrites `expected/` after a
deliberate change. The QEMU harness drives the same programs through
the service console (`qir asm`, `crtpgm`, `call`).

A case file holds one or more modules separated by a line of four `=`;
the first module's program is called. Header comments steer the run:
`; replies: C G` answers the inquiries in order, `; dsply: yes no` the
`dsply` replies, `; parms: 'abc' 12 x'C1'` the parameters as the
console's `call` types them.

Every run sees the same world: `QGPL/CUST` (five customers keyed on
`CUSNO` then `NAME`), the data area `*LDA`, `QGPL/COUNTER` (a
`packed(5,0)` area holding 41), the data queues `QGPL/Q1` and
`QGPL/TESTQ`, the system APIs with the clock fixed at 2026-10-21
12:00:00 UTC, and the kit's own programs as `*PGM` objects.

| Cases | Cover |
|---|---|
| ARITH, CVT, IND, LOOP | integer and float arithmetic, `.chk` overflow, conversions, indicators, branches and `switch` |
| DEC | decimal precision and half-adjust, `MCH1210`/`MCH1202`/`MCH1211`, `%DEC`/`%CHAR`, the 63-digit edge |
| STR, DATE | every string instruction, varying strings, `RNX0100`; date arithmetic, formats, `MCH1602` |
| STOR, PTR | storage classes, structures and arrays, space pointers and their conditions, heaps, user space pointers |
| PARM, CALL, ACTGRP, SYSPTR, STACK | parameter passing, `%PARMS`, recursion, procedure and system pointers, dynamic calls, activation groups |
| MON, EXC | monitor selectors and nesting, the function check, every inquiry reply, condition and cancel handlers, `CEESGL`, `CEE9901` and `CEE9902` to the caller |
| MSG, API | program messages and the message, user space, object, job, date, data queue, data area and CEE APIs with their layouts and error codes |
| DTA, IO | the data area, data queue and file I/O instructions, status codes, the information data structure |
| DEBUG, MISC | debug pseudo-instructions, `dsply` replies |

The outcomes are this implementation's; the IBM i oracle for the
handler and pointer cases is the RPG kit under `conformance/oracle/ile/`.
