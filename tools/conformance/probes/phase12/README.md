# Phase 12 probe scripts

Keystroke scripts for `tn5250-driver` (step language in
`tools/tn5250-driver/src/script.rs`). The `*71` scripts ran against the
primary oracle (V7R1M0, telnet as `QSECOFR`) on 2026-11-03, holding
`qs400-primary.lock`; goldens land in
`tools/conformance/golden/verified/oracle71/panels/` (the Section 1
panels) and `.../oracle71/questions/` (the open questions), with their
provenance in each `INDEX.toml`:

```
cargo run -p tn5250-driver -- run --target oracle \
    --golden tools/conformance/golden/verified/oracle71/panels --update --release V7R1M0 \
    --timeout 30000 tools/conformance/probes/phase12/goldens71.tn
```

| Script | What it captures |
|---|---|
| `goldens71.tn` | `DSPFD` (spooled view), `DSPDTAARA`, `DSPUSRPRF *GRPMBR`, `DSPAUTL`, `EDTAUTL`, `WRKAUTL`, `WRKSYSVAL` and option 2 on `QDATFMT` and `QSYSLIBL`, `WRKUSRPRF`, `WRKCFGSTS`, `DSPDEVD`, `DSPSECAUD`, `WRKOUTQ`, `WRKJOBQ`, `WRKWTR`, `WRKJOBSCDE` |
| `goldens71-dspmod.tn` | `DSPMOD` `*BASIC` and `*PROCLIST` over a CL module made in `QSTEST` (its four-line source sent by FTP first; module and member removed after) |
| `goldens71-keys.tn` | `DSPDEVD` and `DSPAUTUSR` after F11, `DSPHDWRSC` of `*CMN`, `*STG`, `*PRC`, `*LWS`, `*CRP` and `*CSA` |
| `questions71.tn`, `questions71b.tn` | `ENDCMTCTL` with a file open under commitment control, a QSH write to a data member, `CHKOBJITG`'s prompt and run (the first pass typed over refused commands; the second presses Erase Input first) |
| `questions71c.tn`, `dspffd-qasychki.tn` | `CHKOBJITG`'s model outfile found through `QSYS2.SYSTABLES` (QSH `db2`, the job's CCSID set to 37 first), then its `DSPFFD` |

Everything made for a capture lives in `QSTEST` and is deleted by the
script; `QSTEST` held `QSMSGDBG` and `QSMSGSRC` before and after. Change
displays (`EDTAUTL`, Change System Value) are left with F12.
