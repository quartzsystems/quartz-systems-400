# SQL tools' captures (pub400, V7R5M0)

Screens and printed output of STRSQL and the SQL commands' prompts,
captured with `tools/tn5250-driver` from the scripts in
`tools/conformance/probes/sql/` (provenance per capture in each
directory's `INDEX.toml`). Masks as in `golden/verified/pub400/`; the
`-date -time` snaps keep the fixed dates and times the queries select.

- `prompts/`: the prompts of RUNSQL, RUNSQLSTM, CRTSQLRPGI, STRSQL,
  STRDBMON, ENDDBMON, PRTSQLINF and DLTSQLPKG, every page with F10
  (`prompts.tn`; `prompts2.tn` fills the required parameter so Page
  Down pages). `command/sys-cpps/tests/sql_prompts.rs` compares them.
  `dspsqlpkg-cpd0030` and `chgsess-cpd0030`: neither DSPSQLPKG nor
  CHGSESS is a command on IBM i ("Command ... not found").
- `strsql/`: Interactive SQL Session Services and each option, Change
  Session Attributes as SELECT output 2 and 3 and the date and time
  formats reshape it, Display Data under the formats, every statement
  prompt, the saved session (`strsql-a.tn` to `strsql-i.tn`).
  `devtools/isql/tests/services.rs` compares them.
- `strsql/print/`: the printed session (F13, option 2, `QSYSPRT`) and a
  query printed with SELECT output 2 (`QPQUPRFIL`), read back with
  `tools/conformance/sql/oracle/SplfText.java` and deleted after.
  pub400 adds a line of its own (its address) after the end line; it is
  left out here.

## What the captures settled

- A saved session (Exit option 1, or a dropped connection) is resumed
  by the next STRSQL with its statements, messages and attributes:
  SQL6102, then SQL6335 when the saved attributes differ from what the
  command gives or defaults (the command's are not applied), then
  SQL7971. Exit option 2 drops it; the next session starts with the
  command's values, `*JOB` shown as the job's (`*YMD`, `'-'`, `':'`).
- Change Session Attributes is one list over pages of rows 5 to 21,
  no attribute split over two pages: SELECT output 2 adds Printer, 3
  the output file (file `QSQLSELECT` in the current library, member,
  option, authority, text); the date separator shows only for `*MDY`,
  `*DMY`, `*YMD`, `*JUL` (and `*JOB`), the time separator only for
  `*HMS`. Enter on a changed shape shows the display again; Enter on
  an unchanged one applies and returns to Session Services without a
  message.
- Option 2 shows Change Printer File (`QSYSPRT`), Enter prints and
  Session Services says SQL6085. Option 3 asks Confirm Remove Entries
  Request; Enter clears and says SQL6086. Option 4 shows Change Source
  File (`QSQLSESS`, the current library, `*FILE`, option 1,
  `*EXCLUDE`).
- Display Data: numbers edited with thousands separators and a trailing
  minus, no zero before the decimal point (`1,234.50-`, `.50`), a
  column as wide as its largest edited value with the sign
  (`DECIMAL(7,2)` 10, `SMALLINT` 7, `INTEGER` 14, `BIGINT` 26), the
  heading over the last digit; floats `1.5000000000000000E+003` (24
  wide, heading at the end); dates 10 (`*ISO`, `*USA`, `*EUR`, `*JIS`),
  8 (`*MDY`, `*DMY`, `*YMD`) or 6 (`*JUL`); times 8 (`*USA` as
  `01:45 PM`); a null `-` in a number's last column. `*COMMA` swaps the
  separators (`1.234,50-`) and makes a comma in a statement a decimal
  point unless a blank follows it (`dec(-1234.5,7,2)` is SQL0104 there).
- F20 shifts by 79 columns but ends the last page on the data's last
  column (width 127: column 49), with QRY1909; F19 back to column 1
  says QRY1908.
- F4 on typed words and F12 leaves the statement in the history with
  SQL6130; a prompt chosen on Select SQL Statement goes back to the list
  on F12. Typing `grant package` and F4 is a syntax error (SQL0104);
  the GRANT and REVOKE prompts are reached from the list.
- The query report: a heading of the job's date, time and `PAGE n`
  ending on the data's last column, the column heading, the rows, `* *
  *  E N D  O F  R E P O R T  * * *`. The session report: the product,
  release, `Interactive SQL Session`, date, time and `Page n`, the
  session's lines at the margin, `* * * * *  E N D  O F  L I S T I N G
  * * * * *`.
