# Save, restore, media, PTF and problem facts from the oracles

Phase 11 captures (2026-10-30). Read-only on both systems except the
save and restore cases, which ran only against `QSTEST` and
`/home/QSTEST` on the primary oracle.

| Directory | What |
|---|---|
| `primary/` | the primary oracle (V7R1M0): the restore rules (`primary/README.md`), part D's print captures |
| `pub400/` | pub400.com (V7R5M0): the `SAVLIB` of the account library, `DSPSAVF`, the refused `RSTLIB`, the cleanup |
| `tool/` | `Save.java` (JDBC and `QCMDEXC`, job-log capture), `run.sh`, the scripts each case ran |

Screen goldens from pub400 (prompts, menus, `DSPSFWRSC`, `DSPSAVF`, the
display commands) are in `golden/phase11/`, their scripts in
`probes/phase11/`.

## State after the session

| System | State |
|---|---|
| Primary | `QSTEST` holds no objects, as found (`CLRLIB` after `ENDJRNPF`, `DLTJRN` and `DLTJRNRCV`); `/home/QSTEST` holds only `data`, `dds`, `oracle`, `out` from 2026-10-22; the 52 spooled files these jobs left in `QPRTJOB` (compile listings, one `DSPMSG`) and the output queue's two were deleted. One `QPJOBLOG` from 06:55 (system time) in the same `QPRTJOB` predates the session's first command and was left. Two inquiries the first jobs sent to `QSYSOPR` before `INQMSGRPY(*DFT)` was set (`CPA7025` from `CLRLIB` on an unsaved receiver, `CPA0702` from the tool's own program) were answered (`I`, `C`) by a program in `QTEMP`; every later inquiry took its default reply |
| pub400 | `{user}B` holds its 60 objects as found; the save file `QSSAVF` was deleted; the spooled-file count is back to 235 |
