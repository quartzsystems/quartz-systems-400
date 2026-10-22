# ILE oracle kit

One RPG program per open question of `docs/ILE_OPEN_QUESTIONS.md`, and
a CL driver, for the real IBM i. Each program writes what it observed,
one line per observation, through `QORACLE` into `QTEMP/ORACLE`; the
driver runs them all with the inquiry replies they need on the system
reply list and copies the file to `/tmp/ile-oracle.txt`.

To capture: upload the members to a source file, `CRTBNDRPG` each
`RPGLE` member and `CRTBNDCL ORACLE`, `CALL ORACLE`, then save
`/tmp/ile-oracle.txt` as `expected/ile-oracle.txt` here. `cargo test
-p conformance --test ile_oracle` compares every line with this
implementation's reading of the same question and reports the ones
that differ; without the file the test reports itself skipped.

| Program | Question | What it prints |
|---|---|---|
| `CVTCHAR` | 1 | `%CHAR` of `1.50`, `0.5`, `100`, `-0.5` |
| `DATEBAD` | 2 | `%STATUS` after an invalid `%DATE` |
| `SUBSTBAD` | 3 | `%STATUS` after `%SUBST` past the end |
| `RESUMEG` | 7 | the result field after reply `G` to a divide by zero |
| `STATMAP` | 8 | `%STATUS` for `MCH1211`, `MCH1210`, `MCH1202`, `MCH0603`, `MCH3601` |
| `REPLYR` | 9 | the result field after reply `R` |
| `FCWHERE` | 10 | whether the caller's `MONITOR` saw the function check |
| `PROMOTE` | 11 | what the monitor sees after a handler returns 30 |
| `CEEFC` | 12 | the feedback codes of `CEEHDLU` and `CEEMRCR` misuse |
| `CANCELTO` | 13 | the message the caller of a cancelled program gets |
| `DEEPREC` | 14 | the identifier a runaway recursion raises |
| `CMDEXC` | 15 (closed) | the message `QCMDEXC`'s `SNDPGMMSG` left on the queue |
| `PTRSTAT` | 4 | a pointer one byte past a static, then past an automatic, dereferenced |
| `DECBLANK` | 5 | `%DEC` of `'1 2'` |
| `DIFFTS` | 6 | `%DIFF` in months of timestamps a month apart, earlier and later time of day |

`QS400_UPDATE_ORACLE_ILE=1` is not a thing: the expected file comes
from the machine only.

## Captured 2026-10-22 on pub400.com (IBM i 7.5)

The compiler there wanted one statement per line, `const` on the
`QORACLE` prototype's parameters (literals cannot be passed by
reference), and a variable zero instead of `/ 0`, which it rejects at
compile time; the sources now have all three. A pub400 user cannot
`ADDRPYLE`, so the driver ran with `INQMSGRPY(*DFT)`: `RESUMEG` and
`REPLYR` ended on the default reply and left no line. To answer them,
sign on interactively, `CRTPF QTEMP/ORACLE RCDLEN(80)`, `ADDLIBLE
CWELLMAN2`, `CALL RESUMEG` and reply `G`, `CALL REPLYR` and reply `R`,
then `DSPPFM QTEMP/ORACLE`. `CEEFC` ended in a function check before
writing anything, and `CMDEXC` got `CPD0031` (`SNDPGMMSG` is not
allowed through `QCMDEXC`), both from the job log. The comparer
matches lines per program, so those gaps shift nothing. The output
file is a hex field, so `CPYTOIMPF` wrote it unconverted (job CCSID
273); `expected/ile-oracle.txt` is the decoded text.
