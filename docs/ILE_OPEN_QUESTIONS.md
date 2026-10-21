# ILE open questions

Behaviour of the runtime, the binder and the exception model that the
IBM documentation leaves ambiguous, with the reading implemented and
the oracle test that settles it. The oracle programs live under
`conformance/oracle/ile/`; an entry closes when the machine's output
is in `expected/` and the test passes.

## Runtime library

1. **`%CHAR` of a decimal with decimal positions.** Implemented: the
   value as `Decimal` prints it, so `1.50` gives `1.50` and `0.5`
   gives `0.5`, negative values with a leading `-`, left-adjusted.
   The machine may print `.5`. Oracle: `CVTCHAR` in the ILE kit
   prints `%CHAR` of `0.5`, `-0.5`, `100`, `1.50`.

2. **Message identifier of an invalid date, time or timestamp.**
   Implemented: `MCH1602`, mapped to RPG status `00112`. The machine's
   MI identifier for a date operation on invalid data is not certain;
   the RPG runtime reports `RNQ0112`. Oracle: `DATEBAD` performs
   `ADDDUR` on an invalid date under `MONMSG MCH0000` and prints the
   identifier.

3. **Message identifier of a string operation out of range.**
   Implemented: `MCH0603` for `%SUBST`, `%SCAN`, `%REPLACE` and
   `%LEN` assignments outside the string, mapped to status `00100`
   nowhere yet (the runtime maps `MCH0603` to `00121`, the array index
   status). Oracle: `SUBSTBAD` prints the identifier the machine sends
   for `%SUBST` past the end.

4. **Pointer arithmetic within static storage.** Implemented: every
   static variable of a program's static area is one space, so a
   pointer to one static may be advanced across its neighbours without
   `MCH0601`; a pointer to an automatic variable is its own space and
   fails one byte past it. Oracle: `PTRSTAT` advances a pointer past
   a static field and dereferences.

5. **`%DEC` of text with embedded blanks.** Implemented: every blank
   is removed before parsing, so `1 2` reads as `12`. The machine may
   reject embedded blanks with `RNQ0105`. Oracle: `DECBLANK`.

6. **Months in `%DIFF` of timestamps on the same day of month.**
   Implemented: the time of day decides whether the last month is
   complete, as for dates where the day decides. Oracle: `DIFFTS`.

## Exception model

7. **Resume (`G`) after a failing instruction.** Implemented: the
   failing operation's result is zero and execution continues at the
   next instruction. Oracle: `RESUMEG` answers the inquiry with `G`
   after a divide by zero and prints the result field.

8. **`*STATUS` ranges for `ON-ERROR` versus message identifiers.**
   Implemented: a `monitor` handler may name a status range, a
   message identifier or a prefix; the runtime maps `MCH1211` to
   `00102`, `MCH1210` to `00103`, `MCH0603` to `00121`, `MCH1202` to
   `00907`, `MCH1602` to `00112`, and the pointer conditions to
   `00222`. Oracle: `STATMAP` forces each condition under `MONITOR`
   and prints `%STATUS`.

9. **Reply `R` to `CEE9901`.** Implemented as `G`: the failing
   instruction is not retried, execution continues after it. The
   machine retries the instruction. Oracle: `REPLYR` answers `R` to
   a divide by zero whose divisor a handler has since changed, and
   prints the result.

10. **Where the function check is sent.** Implemented: `CPF9999` goes
    to the entry the original escape was sent to, so every entry up to
    the control boundary sees it again (`*PSSR` in the failing
    program first). Oracle: `FCWHERE` prints which of two `MONMSG
    CPF9999` ran, in the callee or the caller.

11. **Condition handler result codes 30 and 31 (promote).** Implemented
    as percolate; the new condition token is ignored. Oracle:
    `PROMOTE` promotes `MCH1211` to a user condition and prints what
    the caller's `MONMSG` saw.

12. **Feedback codes of the condition-management APIs.** Implemented:
    `CEE0801` for a procedure pointer that is not a procedure,
    `CEE0809` for `CEEHDLU`/`CEEUTX` of a handler never registered,
    `CEE0810` for a `CEEMRCR` action other than 0. The machine's
    identifiers for these cases are not certain. Oracle: `CEEFC`
    prints the feedback codes of each misuse.

13. **`CEE9901` to the caller of a cancelled program.** Implemented:
    reply `C` ends the entries down to the control boundary and sends
    `CEE9901` as an escape to the boundary's caller, where the search
    runs again (its own function check and inquiry when unhandled).
    The machine sends `CEE9901` for ILE programs and `CPF9999`-based
    escapes for OPM ones. Oracle: `CANCELTO` calls a failing program
    under `MONMSG CEE9901` and prints the message data.

14. **Stack overflow.** Implemented: every entry measures the native
    stack from the first entry call of the job; past the job's budget
    (6 MiB of a job process's 8 MiB main thread, 1 MiB in memory) the entry
    raises `MCH0601` as soon as it starts, so a runaway recursion is
    an addressing violation a monitor can take, and the guard page is
    never reached. The machine's identifier for a stack overflow is
    not certain. Oracle: `DEEPREC` recurses without end under `MONMSG
    MCH0000` and prints the identifier.

15. **`QCMDEXC` from a program.** Implemented: the command is queued
    and the console runs it after the program returns, since Phase 4
    has no command analyzer (`MIGRATE(phase7)`). The machine runs it
    at once. Oracle: `CMDEXC` calls `QCMDEXC` with `SNDPGMMSG` and
    prints what it receives afterwards.

16. **Files in `QTEMP` from the IR.** Implemented: the IR's file I/O
    resolves names in ASP 1 only. Phase 5 gives every job a `QTEMP` of
    its own, but in the temporary ASP, which a program's file I/O does
    not reach (`MIGRATE(phase6)`). Oracle: none; a limit, not a
    reading.

