# Phase 11 probe scripts

Keystroke scripts for `tn5250-driver` (step language in
`tools/tn5250-driver/src/script.rs`). Captured from pub400.com (V7R5M0)
on 2026-10-30, one session at a time with the driver's 500 ms throttle;
the primary oracle has no telnet, its facts are under
`tools/conformance/oracle/save/primary/`. Goldens land in
`tools/conformance/golden/phase11/<area>/` with their provenance in each
area's `INDEX.toml`:

```
cargo run -p tn5250-driver -- run --target pub400 --auto-signon \
    --golden tools/conformance/golden/phase11/prompts --update --release V7R5M0 \
    tools/conformance/probes/phase11/prompts-save.tn
```

Read-only rules the scripts follow: nothing is typed on a menu's option
line, change and action displays are left with F12 or F3, Enter is
never pressed on a prompt, and the only list option typed is 5
(display). Scripts that press Enter on a display command have no
`continue`, so a failed step ends the run; the prompt and menu scripts
never press Enter after `CALL QCMD` (other than `GO`), so they use
`continue` to step over a refusal. After each run, snapshots that
repeat an earlier page of the same display (the message line aside)
were dropped, and a prompt or menu the profile may not use keeps only
its first snapshot, renamed `NAME-refused`.

Counts after pruning (`.txt` grids, each with its `.attr`): `licpgm`
173, `prompts` 126, `menus` 45, `savf` 13, `displays` 12.

The pub400 profile is refused (`Not authorized to ...`, kept as
`NAME-refused`): `GO LICPGM`, `GO SERVICE` (`GO CMDLICPGM` and `GO
CMDMED` do not exist: "Object ... not found", kept the same way), every restore command (`RSTLIB`, `RSTOBJ`,
`RST`, `RSTUSRPRF`, `RSTAUT`, `RSTCFG`, `RSTLICPGM`), `SAVLICPGM`,
`DLTLICPGM`, `CHKOPT`, every PTF command (`DSPPTF`, `WRKPTFGRP`,
`LODPTF`, `APYPTF`, `RMVPTF`, `SNDPTFORD`, `CPYPTF`, `INSPTF`,
`DSPPTFCVR`), `CHGIPLA`, the problem commands but `DSPPRB`
(`WRKPRB`, `ANZPRB`, `CHGPRB`, `DLTPRB`, `SNDSRVRQS`), the dump and
trace commands, `ADDLICKEY`, `WRKLICINF`, `CHGLICINF`, `PRTSYSINF`,
`RCLDBXREF` and `WRKCNTINF`. The prompts captured are those of
`SAVLIB`, `SAVOBJ`, `SAVCHGOBJ`, `SAV`, `SAVSYS`, `SAVSECDTA`, `SAVCFG`,
`SAVSAVFDTA`, `CRTSAVF`, `CHGSAVF`, `CLRSAVF`, `DSPSAVF`, `DSPTAP`,
`DSPOPT`, `CHKTAP`, `INZTAP`, `INZOPT`, `DUPTAP`, `DUPOPT`, `CPYTOTAP`,
`CPYFRMTAP`, `CRTIMGCLG`, `ADDIMGCLGE`, `LODIMGCLG`, `VFYIMGCLG`,
`WRKIMGCLG`, `CRTDEVTAP`, `CRTDEVOPT`, `CRTDEVMLB`, `DSPSFWRSC`,
`DSPIPLA`, `CHGCLNUP`, `STRCLNUP`, `CHGBCKUP`, `RUNBCKUP`, `DSPBCKUP`,
`DSPPRB` and `WRKSFWAGR`. The primary oracle gave the refused PTF and
problem displays in print form instead (`oracle/save/primary/31`..`35`).
`DSPSAVF` option 5 is valid only on a file (`Option 5 not valid for
specified entry` on the journal).

| Script | Area | What it captures |
|---|---|---|
| `licpgm.tn` | `licpgm` | `GO LICPGM` (refused), `DSPSFWRSC` with every page (2372 rows) and its F11 views, the `DSPSFWRSC` prompt |
| `prompts-save.tn` | `prompts` | F4 prompts with F10 and their pages: `SAVLIB`, `SAVOBJ`, `SAVCHGOBJ`, `SAV`, `SAVSYS`, `SAVSECDTA`, `SAVCFG`, `SAVSAVFDTA`, `SAVLICPGM`, `RSTLIB`, `RSTOBJ`, `RST`, `RSTUSRPRF`, `RSTAUT`, `RSTCFG`, `RSTLICPGM`, `DLTLICPGM`, `CRTSAVF`, `CHGSAVF`, `CLRSAVF`, `DSPSAVF` |
| `prompts-media.tn` | `prompts` | `DSPTAP`, `DSPOPT`, `CHKTAP`, `CHKOPT`, `INZTAP`, `INZOPT`, `DUPTAP`, `DUPOPT`, `CPYTOTAP`, `CPYFRMTAP`, `CRTIMGCLG`, `ADDIMGCLGE`, `LODIMGCLG`, `VFYIMGCLG`, `WRKIMGCLG`, `CRTDEVTAP`, `CRTDEVOPT`, `CRTDEVMLB` |
| `prompts-svc.tn` | `prompts` | `DSPPTF`, `WRKPTFGRP`, `LODPTF`, `APYPTF`, `RMVPTF`, `SNDPTFORD`, `CPYPTF`, `INSPTF`, `DSPPTFCVR`, `DSPSFWRSC`, `DSPIPLA`, `CHGIPLA`, `CHGCLNUP`, `STRCLNUP`, `CHGBCKUP`, `RUNBCKUP`, `DSPBCKUP`, `WRKPRB`, `ANZPRB`, `DSPPRB`, `CHGPRB`, `DLTPRB`, `SNDSRVRQS`, `DMPOBJ`, `DMPSYSOBJ`, `DMPJOB`, `STRTRC`, `ENDTRC`, `PRTTRC`, `TRCJOB`, `ADDLICKEY`, `WRKLICINF`, `CHGLICINF`, `WRKSFWAGR`, `PRTSYSINF`, `RCLDBXREF`, `WRKCNTINF` |
| `menus.tn` | `menus` | `GO` of `SAVE`, `RESTORE`, `BACKUP`, `CLEANUP`, `PROBLEM`, `PROBLEM2`, `PTF`, `SERVICE`, `LICPGM`, and the command menus `CMDSAV`, `CMDRST`, `CMDSAVF`, `CMDPTF`, `CMDPRB`, `CMDLICPGM`, `CMDTAP`, `CMDOPT`, `CMDIMGCLG`, `CMDBCKUP`, `CMDCLNUP`, `CMDTRC`, `CMDDMP`, `CMDMED`, `CMDSRV`, `TAPE`, each with its next pages |
| `disp-*.tn` | `displays` | one display command per session: `DSPPTF` (with option 5 on an applied PTF), `WRKPTFGRP`, `WRKPRB`, `WRKCNTINF`, `DSPIPLA`, `DSPBCKUP`, `WRKLICINF`, `WRKSFWAGR`, `WRKIMGCLG` |
| `savf.tn` | `savf` | `DSPSAVF` over the save file of the account library's `SAVLIB` (`oracle/save/pub400/`): pages, F11 views, F24, option 5 on the journal, a program, a physical file and a source file, the help |
