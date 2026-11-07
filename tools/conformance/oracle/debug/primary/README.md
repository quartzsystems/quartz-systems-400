# STRDBG on the primary (V7R1M0)

What QAP's `Debug.AddProgram` answers for a program without debug data
(docs/QAP.md, "Service 11 `Debug`") comes from here.

| File | What |
| --- | --- |
| `prep.s`, `clean.s` | Scripts for `tools/conformance/oracle/save/tool/Save.java` (JDBC, CL through `QCMDEXC`; work library `QSTEST`): CL programs `QAPNODBG` (`DBGVIEW(*NONE)`) and `QAPSRCDBG` (`DBGVIEW(*SOURCE)`) made from one member, and deleted |
| `strdbg.tn` | `tn5250-driver run --target oracle` as `QSECOFR`: `STRDBG` of each from Command Entry, F1 on the message of the first |
| `strdbg.txt` | What the display showed (2026-11-07), the system name masked |
| `strdbg-batch.s`, `strdbg-batch.txt` | The same `STRDBG`s in the JDBC server job |

Facts:

- `STRDBG PGM(QSTEST/QAPNODBG)`, compiled `DBGVIEW(*NONE)`, at a display:
  Display Module Source opens with no source and the message
  `CPF9C15` naming the program on its message line. QS/400's QAP
  `AddProgram` answers `Error` with `CPF9C15`.
- The same source compiled `DBGVIEW(*SOURCE)` shows its four lines.
- In a batch job (the JDBC server job) `STRDBG` of either is refused
  with `CPF9C58` (escape, severity 20): ILE debug needs an interactive
  job there. QAP's connection jobs debug as QS/400's debugger does in
  any job; the debugger behind it does not refuse that.
