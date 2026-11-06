# RCVMSG on the primary (V7R1M0)

`rcvmsg.s` is a script for `tools/conformance/oracle/save/tool/Save.java`
(JDBC to the primary as `QSECOFR`, CL through `QCMDEXC`; work library
`QSTEST`). It makes two CL programs in `QSTEST`, calls them, and deletes
them; `rcvmsg.txt` is what came back (2026-11-06), the CL programs'
`CPF9897` lines to `*EXT` holding the variables after each `RCVMSG`:

- A1: a receive that finds nothing blanks `MSGID`, `MSG`, `MSGDTA`,
  `KEYVAR`, `RTNTYPE`, `ALROPT`, `SENDER` and zeros `SEV`, `TXTCCSID`,
  `DTACCSID`, whatever they held before.
- B1, the `S...` lines: a message from `QCPFMSG` gives `RTNTYPE` 04,
  `ALROPT` `*NO`, `TXTCCSID` 65535, `DTACCSID` 65535; `SENDER` (20
  characters a line) holds positions 1-26 blank (the sending job, this
  job), 27-38 the sending program, 39-42 `0000`, 43-49 the date
  `CYYMMDD`, 50-55 the time, 56-65 the receiving program, 66-69 `0000`,
  70 and 71 `1` and `1` (ILE), 72-77 microseconds, 78-87 the sending
  user; 88-720 blank.
- C1: an immediate message (`SNDPGMMSG MSG()`) gives `MSGID` blank,
  `RTNTYPE` 04, `ALROPT` blank, `TXTCCSID` 37, `DTACCSID` 0.
- E1, E2: `PGMQ(*SAME RCVT1)` from the called program receives the
  caller's message; so does `PGMQ(*PRV *)`.

`CHGVAR` of a `*DEC` into `*CHAR(5)` gives `00000` and `65535`: right
aligned with leading zeros.
