# Save, restore, media, PTF and problem facts from the oracles

Phase 11 captures (2026-10-30). Read-only on both systems except the
save and restore cases, which ran only against `QSTEST` and
`/home/QSTEST` on the primary oracle.

| Directory | What |
|---|---|
| `primary/` | the primary oracle (V7R1M0): the restore rules (`primary/README.md`), part D's print captures |
| `tool/` | `Save.java` (JDBC and `QCMDEXC`, job-log capture), `run.sh`, the scripts each case ran |
