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

## Arrays (ar1, ar2, and the kit cases `sqlpl/ARRAYS`, `sqlpl/ARRAYERR`)

- `CREATE TYPE t AS type ARRAY[n]` and `ARRAY[]` (cardinality 2147483647);
  `ARRAY` without brackets is SQL0104. SYSTYPES: METATYPE `A`, SOURCE_SCHEMA
  `QSYS2`, MAXIMUM_CARDINALITY, LAST_ALTERED null.
- Arrays live in SQL procedures only: an array parameter of a function, and
  `ARRAY[...]`, `CARDINALITY(ARRAY[...])`, `ARRAY_AGG` outside a routine are
  SQ20441 (428H2). `DECLARE a INTEGER ARRAY[3]` is SQL0199: a variable needs
  a type made by CREATE TYPE. `ARRAY_EXISTS` is not on 7.1 (SQL0199).
- A declared array is null (`CARDINALITY` null, elements null). `SET a[5]`
  past the cardinality extends it with nulls. `TRIM_ARRAY(a, n)` drops the
  last n. `ARRAY_AGG` of no rows is an empty array. `UNNEST(a, b)` pads the
  shorter with nulls; `WITH ORDINALITY` adds the ordinal (BIGINT). Assigning
  an array copies it.
- SQ20439 (2202E): reading past the cardinality, assigning at 0 or past the
  type's cardinality, `TRIM_ARRAY` of more than there are, an
  `ARRAY[fullselect]` longer than the type; at `CREATE PROCEDURE` time for a
  constant index. A null index: null when read, SQ20439 with 2200E when
  assigned at. A fractional index is truncated. A constructor longer than
  the type is SQ20440 (2202F) at `CREATE PROCEDURE`.
- An element out of the element type's range: SQL0304 (22003); a string
  element into an INTEGER array converts.
