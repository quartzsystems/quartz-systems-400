# Calling a program without the authority to (pub400, V7R5M0)

Captured 2026-11-06 on pub400.com as the account `{user}` (no special
authorities), in the account's own library `{user}1` only, with
`tools/conformance/probes/rel/callaut-pub400.tn` and
`callaut2-pub400.tn`. Cody's V7R1M0 box was not used: its only profile
is `QSECOFR`, whose `*ALLOBJ` passes every check, and no profile may be
made there.

Setup: `CRTDUPOBJ OBJ(QSTESTPUB) FROMLIB({user}1) OBJTYPE(*PGM)
NEWOBJ(QSCALLAUT)`, `RVKOBJAUT ... USER(*PUBLIC) AUT(*ALL)` (CPI2218,
CPC2202), `RVKOBJAUT ... USER({user}) AUT(*ALL)` (CPI2202, CPC2202).
`DSPOBJAUT` then showed only `*PUBLIC *EXCLUDE`. The copy was deleted
at the end of each run (`GRTOBJAUT ... AUT(*ALL)`, `DLTPGM`, CPC2191).

What `DSPJOBLOG OUTPUT(*PRINT)` showed (message ID, type, severity,
sending program):

| Authority of `{user}` to the program | Called from | Messages |
|---|---|---|
| none (`*PUBLIC *EXCLUDE`) | Command Entry, `CALL PGM({user}1/QSCALLAUT)` | `CPD0171` Diagnostic 30 `QCACALL`; `CPF0001` Escape 30 `QCACALL` |
| `*OBJOPR *READ` (`*USE` without `*EXECUTE`) | Command Entry | the same |
| `*EXECUTE` alone | Command Entry | the same |
| none | `CALL QCMDEXC PARM('CALL {user}1/QSCALLAUT' 24)` | `CPD0171` and `CPF0001` from `QCACALL`, then `CPF0006` Escape 30 `QCMDEXC` |
| none | a CL program (`CRTBNDCL`, statement 200 `CALL PGM({user}1/QSCALLAUT)` with `MONMSG MSGID(CPF0000 MCH0000)`) | `CPD0171` Diagnostic 30 and `CPF0001` Escape 30 from `QCLCLCPR`, to the CL program, which monitored it |

`CPD0171` names the program and its library (two `*CHAR 10` values);
`CPF0001` names the command (`CALL`). The message line at MAIN shows
the `CPD0171`. A program call therefore needs `*OBJOPR` and
`*EXECUTE` to the program: neither alone is enough.

Not settled here (IBM's documentation followed): `*EXECUTE` to a named
library (the Security Reference's table for `CALL`), `*EXECUTE` to a
service program activated for the program, and what an RPG or C
dynamic call receives (the machine exception `MCH1001`).
