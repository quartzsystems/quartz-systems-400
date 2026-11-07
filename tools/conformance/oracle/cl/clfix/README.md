# CHGVAR, RMVMSG and CALL on the primary (V7R1M0)

`clfix.s` is a script for `tools/conformance/oracle/save/tool/Save.java`
(JDBC to the primary as `QSECOFR`, CL through `QCMDEXC`; work library
`QSTEST`). It makes CL programs `CLFX1` and `CLFX2` in `QSTEST`, calls
them, and deletes them; `clfix.txt` is what came back (2026-11-06), the
`CPF9897` lines to `*EXT` holding what each statement left.
`clfix-cpf0819.txt` is the first run, without the `MONMSG` after the
too-small target: `CPF0819` ended the program there.

- `CHGVAR` of a decimal into `*CHAR`: the value right-aligned, padded on
  the left with zeros, a decimal point when it has decimal places, a
  minus sign in the leftmost position when negative: `12.34` (5 2) in
  `*CHAR 8` is `00012.34`, in 9 `000012.34`; `-12.34` in 8 is
  `-0012.34`; `-5` (3 0) in 5 is `-0005`; `0` (2 0) in 1 is `0`; the
  literal `12` in 5 is `00012`. Leading zeros of the value may go:
  `12.34` (5 2) in `*CHAR 5` is `12.34`. When the significant digits do
  not fit (`1234567` in `*CHAR 3`) the statement escapes with `CPF0819`
  and the variable keeps its value.
- `RMVMSG PGMQ(*SAME CLFX1) CLEAR(*ALL)` in `CLFX2`, called by `CLFX1`,
  removes `CLFX1`'s messages: `CLFX1`'s `RCVMSG PGMQ(*SAME)` after it
  finds nothing.
- `CALL` of a program that does not exist, in a CL program: diagnostic
  `CPD0170` (`Program NOPGM157 in library QSTEST not found.`), then
  escape `CPF0001` (`Error found on CALL command.`), both from
  `QCLCLCPR` to the calling program; `MONMSG CPF0000` catches it. From
  `QCMDEXC` the same two, then `CPF0006`.
