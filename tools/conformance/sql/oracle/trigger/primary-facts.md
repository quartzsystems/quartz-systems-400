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
