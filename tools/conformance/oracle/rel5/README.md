# Release probe: storage thresholds, power and system values

Captured on the primary (V7R1M0, as QSECOFR, read-only) on 2026-11-06
for the V1R1M0 items D1, D6 and D7 with `tools/conformance/jtopen/src/RelProbe.java`
(JTOpen, run directly against the oracle with `QS_HS_*` set from its
credentials) and the `ENDSYS` definition also on pub400 (V7R5M0).

| File | What |
|---|---|
| `sysvals-primary.txt` | `RelProbe sysval ...`: `QWCRSVAL`'s type, length and value of each system value |
| `messages-primary.txt` | `RelProbe msg ...`: each `QCPFMSG` description's severity and its highest substitution variable (the IBM text was read, not kept) |
| `commands-primary.txt` | `RelProbe cmd ...`: `QCDRCMDD` for `RCLSPLSTG`, `STRASPBAL`, `ENDASPBAL`, `CHKASPBAL`, `ENDSYS`, `ENDSBS`, `ADDDSKUNIT`, reduced to keywords, types, ranges, special values and dependencies |

Facts relied on:

- `ADDDSKUNIT` is not a command on V7R1M0 (`QCDRCMDD` answers `CPF9801`)
  nor on V7R5M0 (`ibm-cmd-list.txt`): disk units are added in SST and DST only.
- `ENDSYS` has no `BCHTIMLMT` on V7R1M0 nor on V7R5M0 (pub400's `QCDRCMDD`:
  `OPTION`, `DELAY`, `ENDSBSOPT`, `CONFIRM`). `ENDSBS` has it: no default,
  5 to 9999 minutes or `*NOMAX`.
- `QSTGLOWLMT` comes back from `QWCRSVAL` as binary 50000 for 5.0000 percent
  (four decimal places); `DSPSYSVAL` shows `5.0000`.
- `QUPSDLYTIM` is two elements: the value as set and the delay in seconds
  (`DSPSYSVAL`: `Delay time 200`, `Delay value in seconds 200`).
- `QSVRAUTITV` is binary, 2880; `QBOOKPATH` 315 characters (5 x 63);
  `QMLTTHDACN` `2`; `QCFGMSGQ` `QSYSOPR   QSYS`.
- The `QIBM_QWC_QSTGLOWACN` exit point: format `STGL0100`, no deregistration,
  no change, no maximum, preprocessing programs `QSYS/QWCPDAUT`
  (`../net/pub400/exit-points.txt`); IBM documents no parameters for its
  exit programs.

The `DSPSYSVAL` displays are goldens in
`tools/conformance/golden/verified/oracle71/sysval/`
(`tools/conformance/probes/rel/sysval-power-primary.tn`).
