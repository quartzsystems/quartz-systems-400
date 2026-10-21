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
