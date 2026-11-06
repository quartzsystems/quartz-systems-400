# CCSID sweep of the primary oracle

`sweep.txt` came from the primary (V7R1M0) on 2026-11-06 through
`tools/conformance/jtopen/src/CcsidProbe.java` over `jdbc:as400`:

```
CcsidProbe 37 273 277 278 280 284 285 297 500 871 1140 1141 1142 1143 1144 1145 \
  1146 1147 1148 1149 870 875 1025 1026 420 424 367 819 850 1252 p420 p424 \
  u1200 u1202 u13488 u1208 u61952 u1232 u1234 u1236
```

For each single-byte CCSID the 256 bytes 00 to FF, a `FOR BIT DATA`
value cast to the CCSID (a tag, no conversion), are converted to UTF-16
(CCSID 1200): the `u` lines are the 256 UTF-16 units in byte order; the
`b` lines are those characters converted back to the CCSID. A byte the
system has no character for comes back as U+001A and then 3F.

What the sweep settled:

- 37, 273, 277, 278, 280, 284, 285, 297, 500, 871, 1140 and 1141 match
  QS/400's tables (`storage/ccsid/cp/`) byte for byte.
- 870, 1025, 1026 and 1142 to 1149 map all 256 bytes and round-trip;
  875 has six bytes without a character (DC, E1, EC, ED, FC, FD).
- 420 (Arabic) and 424 (Hebrew) are bidirectional: converted as a
  string, the system reorders and reshapes the text (the `u` lines of
  `ccsid 420`/`ccsid 424` come out reversed). The `per byte` sections
  (`p BYTE UTF16 BACK`) convert each byte alone. QS/400 converts 420
  and 424 a character at a time with these tables and does no bidi
  layout. In 420 several shapes of one letter decode to the same
  character and encode back to the one byte the oracle's `BACK` gives
  (the other rows of `cp0420.csv` are marked decode only).
- The ASCII CCSIDs 367, 819, 850 and 1252 are refused in a `CAST ...
  CCSID` clause on V7R1M0 (SQL0189), so they were not swept.
- Unicode: 1200, 13488, 61952 (UCS-2) and 1208 convert; 1202, 1232,
  1234 and 1236 are refused (SQL0189).

`storage/ccsid/tests/oracle.rs` checks every swept table against this
file.

The Spanish language feature (2931, `QSYS2931`): a job with
`LANGID(ESP) CNTRYID(ES) CCSID(65535)` has default CCSID 284 on the
primary (`DSPJOB OPTION(*DFNA)`, `tools/conformance/probes/rel/timzon71.tn`,
2026-11-06), the CCSID `storage/ccsid/src/nls.rs` gives ESP; 1145 (284
with the euro) is a CCSID a job may choose, not the language's default.
