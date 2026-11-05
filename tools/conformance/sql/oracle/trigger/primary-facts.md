# External triggers on the primary (V7R1M0), 2026-11-05

Probes `p1.sql`, `p2.sql`, `p3.sql` here, run with `TrgProbe.java` (JDBC plus
the data queue and job log servers); outputs `p1.out`, `p2.out`, `p3.out`. The
trigger program is a CL program (`TRGDUMP`, source in the probes) that sends its
buffer to data queue `QSTEST/TRGQ`, refuses a new record starting `X` with
`CPF9898` and, for one starting `R`, changes the byte to `Z`.

## The trigger buffer

Parameters: the buffer, then its length as `BINARY(4)`.

| Offset | Size | Field | Seen |
|---|---|---|---|
| 0 | 10 | file | `TRGT`, blank padded, CP037 |
| 10 | 10 | library | |
| 20 | 10 | member | |
| 30 | 1 | event | `1` insert, `2` delete, `3` update, `4` read |
| 31 | 1 | time | `1` after, `2` before |
| 32 | 1 | commit lock level | `0` with no commitment control |
| 33 | 3 | reserved | x'000000' |
| 36 | 4 | CCSID | 37 |
| 40 | 4 | RRN | 0 for a before insert; the record's number otherwise |
| 44 | 4 | reserved | 0 |
| 48 | 4 | offset of the old record | always 96 |
| 52 | 4 | its length | the record length |
| 56 | 4 | offset of the old null map | old record + record length |
| 60 | 4 | its length | one byte per field, null-capable or not |
| 64 | 4 | offset of the new record | see below |
| 68 | 4 | its length | |
| 72 | 4 | offset of the new null map | new record + record length |
| 76 | 4 | its length | |
| 80 | 16 | reserved | 0 |

The offsets and lengths are present for every event. The new record starts at
the old null map's end rounded up to 16, plus 16; the buffer ends at the new
null map's end rounded up to 16, plus 16 (record lengths 22, 104, 20, 5 with
4, 2, 20, 1 fields: totals 192, 352, 224, 160). An image that does not apply
(the old one of an insert, the new one of a delete or read) and all padding
are x'40'. Null map bytes are `0`/`1` (x'F0'/x'F1').

## Firing

- Before triggers fire in the order added, then after triggers; a before
  trigger with `ALWREPCHG(*YES)` that changes the new record passes the change
  on to the next trigger and to the file.
- `TRGUPDCND(*CHANGE)`: an update that changes nothing does not call it.
- Read triggers fire for each row an SQL `SELECT` reads; their old record is
  the row.
- An insert through an SQL view fires the base table's triggers.
- A trigger ending in an escape: the job log has the escape, then `CPF502B`
  (a notify). SQL gives SQLCODE -443, SQLSTATE 38501, and the JDBC message is
  the program's message (`[CPF9898] refused.`). `CPYF` gets the notify
  unhandled (`CPF5104` cancel reply), stops with `CPF2972`, the records before
  it copied.
- An after trigger refusing leaves its row in a table without commitment
  control; the statement stops there (later rows are not inserted).
- The program deleted after the add: SQL gives SQLCODE -204 (`SQL0204`,
  `TRGDUMP in QSTEST type *PGM`), the job log `CPF4160`. The trigger stays
  operative.

## Commands

- `ADDPFTRG`: `CPI321A` (trigger &1 in library &2 added to file &3 in library
  &4) and `CPC3203`. A trigger name used: `CPD32E7` then `CPF32C6` (operation
  code 1). `RPLTRG(*YES)` replaces it. A program not found: `CPF9811` then
  `CPF32C6`. A view: `CPF3210` then `CPF32C6`. `TRGTIME(*BEFORE)` with
  `TRGEVENT(*READ)`: `CPD730A`, `CPF0001`, `CPF0006`. `TRGUPDCND` with an
  event other than `*UPDATE`: `CPD7308`, `CPF0001`, `CPF0006`. `ALWREPCHG(*YES)`
  is accepted on after triggers, the read trigger included.
- No `TRG`: the name is `QSYS_TRIG_` + library padded with `_` to 10 + `_` +
  file padded to 10 + `_` + six digits (`QSYS_TRIG_QSTEST_____TRGT_______000001`).
- `ACTION_ORDER` (`SYSTRIGGERS`): the file's count of triggers added, from 1;
  a failed add takes no number, `RPLTRG(*YES)` keeps the trigger's number and
  uses one (A1 1, A2 2, A1 replaced, A3 4).
- `RMVPFTRG`: by `TRGTIME`/`TRGEVENT` (default all), or `TRG`; each removed
  `CPI321B` (trigger &1 in library &2 removed from file &3 in library &4),
  then `CPC3205`. A name not there: `CPD321B` then `CPF32C6` (operation 2);
  none at the time and event: `CPD32E7` then `CPF32C6` (`p4.sql`).
- `CHGPFTRG`: `CPI32E8` per trigger changed, then `CPC32E8` naming the file.
- `CHGPFTRG TRG(*ALL) STATE(*DISABLED|*ENABLED)`; a named trigger needs
  `TRGLIB` (`CPD32E9`, `CPF0001`, `CPF0006` without it). A name not there:
  `CPF32C6`. A disabled trigger is not called; `SYSTRIGGERS.ENABLED` is `N`.
- `DSPFD TYPE(*TRG) OUTPUT(*OUTFILE)`: the `QAFDTRG` record as in `p1.out`
  (TRTRTM `B`/`A`, TRTREV `I`/`U`/`D`/`R`, TRTRCN blank, `A` always, `C`
  change, TRTARC `Y`/`N`, TRTTSF `U`, TRTMJA `S`, TRNAM 258, TRST `E`, TRSTAT
  `O`, TRTYP `Y`, TRORNT `R`).

# SQL triggers on the primary (V7R1M0), 2026-11-05

Probes `p5.sql` and `p6.sql` here, run with `TrgProbe.java`; outputs `p5.out`
and `p6.out`.

## The program

- `CREATE TRIGGER` makes a `*PGM` in the trigger's schema, named as an SQL
  procedure's: the trigger's name when it is a system name no program has,
  else five characters and five digits (`TRIGGER_WITH_A_LONG_NAME_A1` is
  `TRIGG00001`; `CREATE OR REPLACE TRIGGER TRC1` while `TRC1` exists makes
  `TRC100001`). `SYSPROGRAMSTAT` shows it with `ROUTINE_TYPE` `TRIGGER`.
- `DROP TRIGGER` deletes the program; `DROP TABLE` deletes its triggers'
  programs. A second `DROP TRIGGER`: SQLCODE -204, 42704.
- `CPYF` into the table fires the SQL triggers as an insert does (the
  before trigger changed the record, both after triggers ran); the
  statement trigger ran once for the copy.
- `CHGPFTRG STATE(*DISABLED)` disables an SQL trigger like any other
  (`SYSTRIGGERS.ENABLED` `N`); `RMVPFTRG TRG(name)` removes one
  (`CPI321B`).

## Firing

- A `BEFORE` trigger setting `N.col` changes the row written.
- `MODE DB2ROW` after triggers fire as each row changes (`COUNT(*)` of the
  table 1, 2, 3 over a three-row insert); `MODE DB2SQL` ones (the default
  for `AFTER`) once every row has changed (3, 3, 3), after the `DB2ROW`
  ones; the statement trigger last.
- A body ending in an error, `SIGNAL` included: SQLCODE -723, SQLSTATE
  09000, `SQL0723` "SQL trigger TRB4 in QSTEST failed with SQLCODE -438
  SQLSTATE 75001" (the message text is the body's: `negative amount`;
  for `1 / 0`, -802 22012). The job log has the body's message, `SQL0723`,
  `CPF502B` twice, `SQL0723`.
- An after trigger failing on the third row of a four-row insert leaves
  three rows (no commitment control).
- `UPDATE OF NAME`: an update of another column does not fire it.

## Definitions

- `BEFORE` with no `MODE`: warning SQLCODE 7051, SQLSTATE 01647, made
  `DB2ROW`. `AFTER` with no mode is `DB2SQL`; `INSTEAD OF` `DB2SQL`.
- A name in use: -601, 42710. `CREATE OR REPLACE TRIGGER` is there on 7.1;
  the replaced trigger keeps its `ACTION_ORDER`.
- `REFERENCING OLD` on an insert trigger: -696, 42898 (reason 2); a
  transition table with `BEFORE` or `DB2ROW`: -696 (reason 3).
- `FOR EACH STATEMENT` with `BEFORE` or `MODE DB2ROW`: -628, 42613.
- `INSERT` in a `BEFORE` trigger: -751, 0W000.
- `SET N.col` in an `AFTER` trigger: -312, 42618, naming the column.
- `INSTEAD OF` on a table: -159, 42809.
- `LABEL ON TRIGGER` sets `TRIGGER_TEXT`, `COMMENT ON TRIGGER`
  `LONG_COMMENT`.

## Catalogs

- `SYSTRIGGERS` (7.1 has no `SQL_NAMING`): `ACTION_CONDITION` is the `WHEN`
  condition with its parentheses and `ACTION_STATEMENT` the statement, each
  token one blank apart with a blank at the end (`( N . AMT < 0 ) `);
  `ACTION_REFERENCE_OLD_ROW`/`NEW_ROW` the correlation names;
  `TRIGGER_PROGRAM_NAME`/`LIBRARY` the program; SQL triggers are
  `THREADSAFE` `YES`, `MULTITHREADED_JOB_ACTION` `RUN`, a `BEFORE` one
  `ALLOW_REPEATED_CHANGE` `YES`, `ROUNDING_MODE` `E`.
- `SYSTRIGCOL`: one row per read of a transition variable (`N.NAME` read
  once is one row; `O.ID` in the `WHEN` and in the body is two; a `SET`
  target is none), and every column of a table the body inserts into
  without a column list; `OBJECT_TYPE` `TABLE`.
- `SYSTRIGDEP`: one row per table the statement names (a subquery's
  included), `OBJECT_TYPE` `TABLE` padded to 24.
- `SYSTRIGUPD`: one row per `UPDATE OF` column (`TRIGGERED_UPDATE_COLUMNS`).
