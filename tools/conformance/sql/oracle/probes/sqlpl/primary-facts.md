# Savepoints and ATOMIC compounds on the primary (V7R1M0), 2026-11-05

Probes `sp1.sql`, `sp2.sql`, `sp3.sql` here, run with `PlProbe.java` (the kit's
statements plus `-- autocommit`, `-- isolation` and `-- cmd` lines); outputs
`sp1.out`, `sp2.out`, `sp3.out`. The table is journaled with `CRTJRN`/`STRJRNPF`
in QSTEST and dropped after.

## Savepoints (sp1, and the kit case `sqlpl/SAVEPT`)

- `ON ROLLBACK RETAIN CURSORS` is required (`SAVEPOINT A` is SQL0104);
  `ON ROLLBACK RETAIN LOCKS` may follow it, not replace it; `UNIQUE UNIQUE`
  is SQL0199.
- A second `SAVEPOINT B` while a `UNIQUE` B is set: SQL0881 (3B501). A second
  non-unique `A` replaces the first.
- `ROLLBACK TO SAVEPOINT B` undoes the changes after B and releases the
  savepoints set after it; B stays (a second rollback to it works).
  `ROLLBACK TO SAVEPOINT` without a name goes to the latest.
- `RELEASE [TO] SAVEPOINT B` releases it; an unknown or released name is
  SQL0880 (3B001) for both statements.
- `COMMIT` and `ROLLBACK` release every savepoint; then `ROLLBACK TO
  SAVEPOINT` without a name is SQL0882 (3B502).
- Under autocommit with no commitment control (`SAVEPT`), savepoints are
  accepted and kept by name: rolling back to one succeeds and undoes nothing.

## ATOMIC compounds (sp2, sp3)

- `COMMIT` in an ATOMIC compound: SQL0775 (42910) at `CREATE PROCEDURE`.
  An UNDO handler in a compound that is not ATOMIC: SQL0780 (428D6).
  `SAVEPOINT`/`ROLLBACK TO SAVEPOINT` inside one are allowed.
- An exception no handler takes ends the procedure and undoes the ATOMIC
  compound, also with `SET OPTION COMMIT = *NONE` (B1), under autocommit and
  under `*CHG` (B1, B2 with 1, 2, 11, 12 absent).
- An exception taken by a handler of an enclosing compound does not undo the
  ATOMIC compound (B4: 4 kept, then 1004 inserted).
- An UNDO handler undoes the compound, runs, and leaves the compound (B3: 3
  absent, 53 present, 73 absent; B6: 6 absent, 56 and 7 present).
- An EXIT handler inside an ATOMIC compound keeps its changes (A6: 600).
- Over a table that is not journaled, a change in an ATOMIC compound is
  SQL7008 (the kit case `sqlpl/ATOMIC`).
- Without an ATOMIC compound, a failing procedure's earlier changes stay (B5).
