# Time zone descriptions on the primary

Captured from the primary (V7R1M0, telnet as `QSECOFR`) on 2026-11-06,
read-only (no zone made, changed or deleted, no system value changed).

- `zones.tsv`: every `*TIMZON` IBM ships (93), in `WRKTIMZON` order, as
  Display Time Zone Description Details shows it: offset, standard and
  daylight names, message identifiers (`CPX09xx` in `QCPFMSG`), message
  file, the daylight saving start and end (month, day, relative day,
  local time), the shift, the year offset, the alternate name and the
  text. Made by `tools/conformance/probes/rel/timzon71-details.tn`
  (option 5 on every row, the pages printed with `show`) and
  `parse.awk`. The full names are IBM's message texts and are here only
  as the record of what the oracle showed; QS/400's names are its own
  (`lic/sysval/zones.toml [messages]`, the `CPX09xx` entries of
  `runtime/msgf/msgs/qcpfmsg.toml`).
- The panels and prompts: `tools/conformance/golden/verified/oracle71/timzon/`
  (`probes/rel/timzon71.tn`, `timzon71b.tn`).

What the capture settled:

- The commands are `CRTTIMZON`, `CHGTIMZON`, `DLTTIMZON` and
  `WRKTIMZON`. `DSPTIMZON` does not exist (`Command DSPTIMZON in library
  *LIBL not found.`); the details display is `WRKTIMZON` option 5.
- `CRTTIMZON` parameters: `TIMZON`, `OFFSET` (-779 to 779 minutes),
  `STDNAME` (abbreviation `*GEN`/`*MSG`/a value, full name), `DSTNAME`
  (`*NONE`/`*MSG`/a value, full name), `STDMSG`, `DSTMSG`, `MSGF`,
  `DSTSTR` and `DSTEND` (month `*JAN`..`*DEC`, day `*MON`..`*SUN`,
  relative day `*LAST`/1-4, time) and `DSTSHIFT` (0 to 120, default 60)
  once a daylight abbreviation is given, `TEXT`, `YEAROFS` (-140 to 140),
  `ALTNAME`, `AUT` (`*USE`). `DLTTIMZON TIMZON` takes a generic name;
  `WRKTIMZON TIMZON` defaults to `*ALL`.
- A missing description: `Object QSNOSUCH in QSYS type *TIMZON not
  found.` (`CPF2105`).
- `WRKTIMZON` marks the zone `QTIMZON` names with `>`; its options are
  2, 4, 5, 6 and 8, F11 cycles standard names, daylight names,
  alternate names and text.
- `DSPSYSVAL QTIMZON` shows the zone's offset, full and abbreviated
  names under "Associated settings".
- `sql.txt`: the primary's catalog has no view, table or routine with
  TIME and ZONE in its name (no `QSYS2.TIME_ZONE_INFO` at V7R1M0), and
  `CURRENT TIMEZONE` is `DECIMAL(6,0)` hhmmss (-50000 under `QN0500EST`).
  pub400 (V7R5M0) was not asked: no pub400 credentials were in the
  oracle environment. QS/400 provides no `TIME_ZONE_INFO`.
- A job with `LANGID(ESP) CNTRYID(ES) CCSID(65535)` has default CCSID
  284 on the primary, the CCSID QS/400's `QSYS2931` keeps.
