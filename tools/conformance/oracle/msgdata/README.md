# Message data layouts from the primary oracle

Captured on 2026-11-18 from the primary oracle (IBM i V7R1M0) for
handoff 21 (T1): the IBM message descriptions the save, restore and
media commands send, so each message QS/400 sends has IBM's identifier
for that meaning and IBM's data layout (the part a program monitoring
for the message reads). The text in `runtime/msgf/msgs/qcpfmsg.toml`
stays Quartz-written; only the identifier, the severity and the fields
are IBM's.

## How

`DSPMSGD RANGE(...) MSGF(QSYS/QCPFMSG) DETAIL(*FULL) OUTPUT(*PRINT)`
over JDBC with `../save/tool/Save.java` (the scripts `dspmsgd*.s`; the
sign-on server answers again, so `QS_SKIP_SIGNON` is not set):

```
cls=target/save-tool; jar=target/jtopen/jt400.jar
java -cp "$cls:$jar" Save tools/conformance/oracle/.env oracle OUT dspmsgd.s
awk -f table.awk OUT/0*-dspmsgd.txt | sort -u
```

A range is in EBCDIC order: letters sort before digits, so `CPF3700`
to `CPF37FF` is refused (`CPF2515`) and is written `CPF37A0 CPF3799`.

Only `fields.txt` is kept: one line per message, `ID|SEV|fields|first-level
text`, a field as `&n TYPE LENGTH` (`*CHAR 0` is a field IBM declares
with no length, a placeholder in the tape messages). The prints
themselves carry IBM's second-level help and stay out of the tree.

## What it settled

- V7R1M0 has no `CPC6773` or `CPC6774`; IBM documents no completion
  message for `INZTAP` of a labeled volume (only `CPC6776` for a
  nonlabeled one) and `CPF6754` as the escape for an active file.
- `CPA4036` is IBM's inquiry for expiration dates out of sequence; an
  active file at a write is `CPA6754`, `Active file &4 found on volume
  &2 (C I)`.
- `CPA4075` is a printer message; the next tape volume is `CPA4088`
  (`CPA4089` when the volume is named).
- `CPF6708` has no data. `CPC2102` is `&1 *CHAR 10; &2 *UBIN 2; &3
  *CHAR 10`; `CPC3703` has ten fields (counts, both libraries, a time
  stamp).
