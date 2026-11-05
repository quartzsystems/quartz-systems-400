# Planner and plan cache probes (pub400, V7R5M0, 2026-11-05)

`SqlProbe` runs of `p1.sql` to `p4.sql`; IBM's message text is left out,
other users' libraries are written `{otherN}`. What the engine takes from
them:

- `p1`: the plan cache procedures (`DUMP_PLAN_CACHE` is `QSYS2/DUMP_PC`,
  `CLEAR_PLAN_CACHE` `QSYS/QQQOOOCSRV(CLEAR_PLAN_CACHE)`,
  `CHANGE_PLAN_CACHE_SIZE` `QSYS2/CHANGE_PC`), the Visual Explain
  procedure `QSYS.QQQDBVE`, and `SYSCOLUMNSTAT`'s 45 columns. There is no
  plan cache view in `QSYS2`.
- `p2`: the procedures' parameters (`QQQDBVE(INPUT_QQJFLD VARCHAR(46),
  INPUT_STRUCT CHAR(72), OUT OUTPUT_RC INTEGER, OUT OUTPUT_ENTRY
  INTEGER)`: monitor data in, an undocumented structure), and
  `SYSCOLUMNSTAT` rows (statistics named `QDBST_` and 32 hex digits,
  created by `*SYS`, `HIGH2KEY`/`LOW2KEY` empty).
- `p3`: `EXPLAIN` is SQL0104; `OPTIMIZE FOR ALL ROWS` and `5 ROW` are
  taken, `0 ROWS` is SQL0405; the plan cache procedures need `*JOBCTL`
  (CPF43A3, SQLSTATE 38501; `CLEAR_PLAN_CACHE` SQL0443 42502);
  `QUERY_TIME_LIMIT` 0 stops a query with SQL0666 57005 (estimated run
  time 1 over a limit of 0), `QUERY_STORAGE_LIMIT` 0 lets a small sort
  run.
- `p4`: one table's `SYSCOLUMNSTAT` rows in full.
