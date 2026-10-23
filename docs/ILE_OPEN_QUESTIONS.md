# ILE open questions

Behaviour of the runtime, the binder and the exception model that the
IBM documentation leaves ambiguous, with the reading implemented and
the oracle test that settles it. The oracle programs live under
`tools/conformance/oracle/ile/`; an entry closes when the machine's output
is in `expected/` and the test passes.

## Runtime library

1. **`%CHAR` of a decimal with decimal positions.** Settled on pub400
   (IBM i 7.5): `%CHAR` shows every declared decimal and no zero before
   the point: `1.50`, `.50`, `100`, `-.50`. Implemented as
   `Decimal::char_text`.

2. **Message identifier of an invalid date, time or timestamp.**
   Implemented: `MCH1602`, mapped to RPG status `00112`. The machine's
   MI identifier for a date operation on invalid data is not certain;
   the RPG runtime reports `RNQ0112`. Oracle: `DATEBAD` performs
   `ADDDUR` on an invalid date under `MONMSG MCH0000` and prints the
   identifier.

3. **Message identifier of a string operation out of range.** The
   machine answers status `00100` (the RPG runtime's `RNX0100`), not
   `00121`. Still open here: the runtime raises `MCH0603` for `%SUBST`,
   `%SCAN` and `%REPLACE` out of range, which the IR kit and `QIR.md`
   describe; moving them to `RNX0100` (the condition exists in
   `codegen::Cond`) changes the STR and MON kit cases with it.

4. **Pointer arithmetic within static storage.** The machine reads one
   byte past a static *and* past an automatic variable without error:
   the stack is one space. Implemented: statics are one space; an
   automatic variable is still its own space here and fails one byte
   past it (`00222`). Open until automatic storage is modelled as the
   entry's frame.

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

10. **Where the function check is sent.** Settled: the caller's monitor
    sees status `00202` (call failed), with the callee's original
    message identifier; implemented in the handler search.

11. **Condition handler result codes 30 and 31 (promote).** Settled:
    after a promote to a user condition the monitor still sees status
    `00102`; a promoted condition without a status of its own keeps the
    one it replaced.

12. **Feedback codes of the condition-management APIs.** Implemented:
    `CEE0801` for a procedure pointer that is not a procedure,
    `CEE0809` for `CEEHDLU`/`CEEUTX` of a handler never registered,
    `CEE0810` for a `CEEMRCR` action other than 0. The machine's
    identifiers for these cases are not certain. Oracle: `CEEFC`
    prints the feedback codes of each misuse.

13. **`CEE9901` to the caller of a cancelled program.** Settled: the
    caller receives the original escape (`MCH1211`) with status `00202`;
    `CEE9901` goes to the external queue only when there is no caller.

14. **Stack overflow.** Settled: the identifier is `MCH4429`, seen at
    status `00202` by a monitor around the call that overflowed; the
    budget check and the guard-page handler raise it.

15. **`QCMDEXC` from a program.** Closed: since Phase 7 the command
    runs at once through the job's analyzer hook, as on the machine.
    The oracle `CMDEXC` stays in the kit to confirm what the program
    sees afterwards (the message `SNDPGMMSG` left on its queue).

16. **Files in `QTEMP` from the IR.** Closed: since Phase 5 each job's
    `QTEMP` is its own library and a program's file I/O resolves it.

