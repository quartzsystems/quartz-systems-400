# Phase 3: the database

What Phase 3 built on the object store: DDS, decimal arithmetic, files
and members with access paths, record-level access, journaling and
commitment control, SQL, the service console commands and the
conformance kit that checks all of it against a real IBM i. The
byte-level formats are in `DB_FORMAT.md`, the decimal rules in
`DECIMAL.md`, and every behaviour we could not settle without the
machine is in `DB_OPEN_QUESTIONS.md` with the test that settles it.
QS/400 is x86_64 only.

## Crates

```
dds/          DDS: src/source/ fixed-column records, src/lex/ columns and
              keywords, src/compile/ field, key, select/omit and file-keyword
              builders, src/pf/ and src/lf/ compilers, src/listing/, src/refs/
qdecimal/     Decimal (63 digits), IBM intermediate precision (src/arith/),
              packed, zoned, binary, integer and float encodings
              (src/encode/), edit codes and words (src/edit/), the oracle
              fuzzer (src/fuzz/, src/bin/qdfuzz.rs)
db/           the database on the store, bottom up:
  src/file/     *FILE objects: attributes, formats, members, dependents,
                create (PF, LF, source, RCDLEN), change, level check
  src/format/   format sections on disk, the level identifier
  src/record/   field values, dates and times, nulls, VARLEN
  src/path/     key encoding and the access-path B+tree
  src/lf/       logical derivation: rename, CONCAT, SST, joins, select/omit
  src/member/   *MBR objects, record slots, path maintenance and rebuild
  src/odp/      open data paths, the RLA operations, record locks, INFDS
                feedback, OVRDBF overrides (ovrdbf.rs), the job session
  src/util/     CLRPFM, RGZPFM, CPYF, CPYTOIMPF/CPYFRMIMPF, chkpath
  src/dsp/      DSPFD, DSPFFD, DSPPFM, DSPDBR
  src/chg/      CHGPF, CHGLF, CHGPF SRCFILE
  src/source/   source members: srcput, srcget, the DDS source of a member
  src/journal/  STRJRNPF/ENDJRNPF, the entries written, APYJRNCHG/RMVJRNCHG
                (undo.rs), IPL recovery (recover.rs)
  src/cmt/      commitment control
jrn/          *JRN and *JRNRCV objects, the 128-byte entry header, receiver
              chains, DSPJRN filters (src/dsp/)
sql/          lexer, AST, parser; src/exec/ context, expressions, functions,
              the planner, DML, DDL as DDS, cursors, the QSYS2 catalogs;
              sqlca.rs, run.rs (RUNSQLSTM), rla.rs (the scripted RLA driver)
slic/         src/svc/db/: files, journal, sqlcmd, rla and bench console
              commands; src/ipl/storage.rs database recovery at IPL
conformance/  src/db/: the kit (kit.rs), the in-memory run (local.rs), the
              comparer (compare.rs), the oracle generator (rpg.rs), the
              QEMU steps (qemu.rs); db/: the QSTEST kit itself
```

## Design

**DDS first.** Every file is made from DDS. SQL `CREATE TABLE`,
`CREATE INDEX` and `CREATE VIEW` build DDS source, hand it to the same
compiler as `CRTPF`/`CRTLF`, and keep the source in the file's
`SourceEntry`, so `DSPFD` shows an SQL table as the physical file it
is. The compiler never touches the store; it resolves `PFILE`, `JFILE`,
`REF`, `REFFLD` and `FORMAT` through a `RefResolver` the database layer
implements. The keyword set is the database subset: `UNIQUE`, `FIFO`,
`LIFO`, `FCFO`, `REF`, `REFACCPTH`, `ALTSEQ`, `CCSID`, `TEXT`, `FORMAT`,
`PFILE`, `JFILE`, `JOIN`, `JFLD`, `JDUPSEQ`, `JDFTVAL`, `JREF`, `DYNSLT`,
`ALL`, `VALUES`, `RANGE`, `COMP`/`CMP`, `CHECK`, `CONCAT`, `SST`,
`RENAME`, `TRNTBL`, `REFFLD`, `COLHDG`, `ALIAS`, `VARLEN`, `ALWNULL`,
`DFT`, `DATFMT`, `DATSEP`, `TIMFMT`, `TIMSEP`, `EDTCDE`, `EDTWRD`,
`FLTPCN`, `DESCEND`, `ABSVAL` and the rest of the key keywords. The
listing names every diagnostic with a `DDSnnnn` identifier and a
severity; a compile that reaches severity 20 creates nothing.

**Decimal.** One `Decimal` of up to 63 digits carries packed, zoned,
binary, integer and float fields alike. Arithmetic follows the RPG
runtime's intermediate precision rules; assignment half-adjusts and
raises size errors. Edit codes and words render as the machine does.
`qdfuzz` writes a kit of cases that an RPG program on IBM i answers; the
answers under `tools/conformance/oracle/qdecimal/` are the test.

**Files, formats, members.** A `*FILE` object holds a 128-byte
`FileAttr`, the member table, the dependents table (which logical
members depend on this physical member) and the record formats as
sections: fields, keys, based-on files, source, select/omit, joins. A
format's level identifier is 52 bits of the SHA-256 of its fields in
buffer order, 13 hexadecimal digits; `LVLCHK` compares it at open. A
`*MBR` object owns the record slots in a byte space and the access path
tree. Records are fixed-length slots with a null map; a deleted slot
keeps its number and `REUSEDLT(*YES)` lets the next write take it.
`SIZE` bounds the member as on the machine: the default
`SIZE(10000 1000 3)` fills at 13,000 records.

**Access paths.** A path is a B+tree owned by the member with `PathKey`
keys: the encoded key, the format number for a multi-format path, then
the RRN, so equal keys keep arrival order and every entry is unique.
Keys encode so that bytes compare the way the fields compare: EBCDIC
collation for text, sign-and-magnitude for numbers, `DESCEND`
complemented, nulls per `*EXCNULL`. A physical member maintains its own
path and, through the dependents table, every `MAINT(*IMMED)` logical
path over it in the same transaction; `MAINT(*REBLD)` paths are marked
invalid and rebuilt at the next open, `MAINT(*DLY)` on the next keyed
open. Logical files derive their records from the based-on record
(rename, `CONCAT`, `SST`), evaluate static select/omit when the path is
built and `DYNSLT` at read time. A join logical file joins each
secondary by its `JFLD` pairs on read, taking the first match and
omitting an unmatched primary record.

**The open data path.** `Odp::open` resolves the member, checks the
level, allocates the file and member locks the usage needs, and gives
the RPG operations: `SETLL`, `SETGT`, `READ`, `READP`, `READE`,
`READPE`, `CHAIN` (by key or RRN), `WRITE`, `UPDATE`, `DELETE`,
`UNLOCK`, `FEOD`, `CLOSE`. Positioning follows the machine's rules
(a `READE` after `SETLL` with a partial key, `READP` from the end, the
cursor after a `DELETE`). Record locks are per job in the shared lock
table; a read for update takes one, `UNLOCK` and the next operation
release it. The INFDS feedback keeps `%STATUS`, `%FOUND`, `%EQUAL`,
`%EOF` and the RRN: `00011` end of file, `00012` not found, `01021`
duplicate key, `01211` not open, `01218` record locked, `01221` update
without a prior read, `01222` allocation failed, `01299` anything else.
`OVRDBF` overrides live in the job session: `TOFILE`, `MBR`, `SHARE`,
`LVLCHK`, `INHWRT`, `POSITION`; `SHARE(*YES)` opens share one ODP
with one cursor, and a share with a different usage is `CPF4123`.

**Utilities.** `CLRPFM`, `RGZPFM` (with `KEYFILE`), `CPYF` with record
selection and `FMTOPT(*MAP *DROP *NOCHK *CVTSRC)`, `CPYTOIMPF` and
`CPYFRMIMPF` with delimited and fixed data, the displays `DSPFD`,
`DSPFFD`, `DSPPFM` (character and hexadecimal) and `DSPDBR`, `CHGPF`
and `CHGLF`, and source members with sequence numbers and dates. Each
allocates what the command allocates and runs in the caller's
transaction.

**Journaling.** A `*JRN` names its attached `*JRNRCV` and its receiver
directory; a receiver holds 128-byte entry headers with their data in
a byte space, and a threshold attaches the next generated receiver.
Entries append in the caller's transaction, so the journal and the data
commit together. `STRJRNPF` records the journal in the file's
attributes; members and open data paths then write `R PT`, `R UP`,
`R DL` (and `R UB` before images under `IMAGES(*BOTH)`), `F OP`/`F CL`
unless `OMTJRNE(*OPNCLO)`, and `F JM`, `F EJ`, `F MC`, `F MR`, `F MA`,
`F MD` for the member events. `APYJRNCHG` replays and `RMVJRNCHG` undoes
a range, marking what it wrote as applied. `DSPJRN` walks the receiver
chain with the IBM filters.

**Commitment control.** `STRCMTCTL` gives the job one commit definition
with a lock level. Opens under it journal every change with the cycle
identifier that the first `C BC` entry's sequence number gives; record
locks are held to the commit boundary per `LCKLVL(*CHG|*CS|*ALL)`;
`COMMIT` writes `C CM`, `ROLLBCK` undoes the cycle from the before
images and writes `C RB`; `ENDCMTCTL` with files open is `CPF8356`. A
journaled file is required (`CPF8351`). At IPL, after the store is
open, slic rolls back every cycle any journal left open and reports
`C1001082` with the count (`db::journal::recover_all`).

**SQL.** The lexer and recursive-descent parser produce an AST of the
DB2 for i subset below. `SqlContext` carries the job, the naming mode
(`*SYS` or `*SQL`), the current schema and the session. The planner
scores the access paths of a table against the predicates and the
`ORDER BY` and picks a keyed scan, a range or a full scan; joins are
nested loops with the inner side probed through a path when one fits;
grouping, `DISTINCT`, ordering and `FETCH FIRST` materialize between
steps. DML runs through the open data path, so journaling, commitment
control and locks are the same as for RPG. Views become select/omit
logical files; the catalogs under `QSYS2` are computed from the store
on every query. The `SQLCA` carries SQLCODE, SQLSTATE, the message
identifier and the row count. `RUNSQLSTM` runs a member with `COMMIT`
and `ERRLVL`.

## The SQL subset

Statements: `SELECT` (joins `INNER`, `LEFT`, `RIGHT`, `EXCEPTION`,
`CROSS`; `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY`, `DISTINCT`, `FETCH
FIRST n ROWS ONLY` and `LIMIT`, `UNION`, `EXCEPT`, `INTERSECT`, `VALUES`,
derived tables, scalar and `EXISTS`/`IN`/quantified subqueries, `CASE`,
`CAST`, `BETWEEN`, `LIKE`, `IS NULL`, labeled durations, the special
registers), `INSERT` (values and subselect), `UPDATE`, `DELETE`,
`MERGE`, `DECLARE CURSOR`, `OPEN`, `FETCH`, `CLOSE`, `UPDATE`/`DELETE
WHERE CURRENT OF`, `CREATE TABLE`, `CREATE INDEX`, `CREATE VIEW`, `DROP
TABLE|INDEX|VIEW`, `LABEL ON`, `COMMENT ON`, `RENAME`, `COMMIT`,
`ROLLBACK`, `SET SCHEMA`, `EXPLAIN` (a query).

Functions: `ABS`/`ABSVAL`, `ADD_MONTHS`, `BIGINT`, `CEIL`/`CEILING`,
`CHAR`, `CHAR_LENGTH`/`CHARACTER_LENGTH`, `COALESCE`/`VALUE`/`IFNULL`,
`CONCAT`, `DATE`, `DAY`, `DAYNAME`, `DAYOFMONTH`, `DAYOFWEEK`,
`DAYOFWEEK_ISO`, `DAYOFYEAR`, `DAYS`, `DEC`/`DECIMAL`, `DIGITS`,
`DOUBLE`/`DOUBLE_PRECISION`, `FLOAT`, `FLOOR`, `HEX`, `HOUR`, `INT`/
`INTEGER`, `JULIAN_DAY`, `LAST_DAY`, `LCASE`/`LOWER`, `LEFT`, `LEN`/
`LENGTH`, `LOCATE`, `LTRIM`, `MAX`, `MICROSECOND`, `MIN`, `MINUTE`,
`MOD`, `MONTH`, `MONTHNAME`, `NEXT_DAY`, `NOW`, `NULLIF`, `POSITION`/
`POSSTR`, `POW`/`POWER`, `REAL`, `REPLACE`, `RIGHT`, `ROUND`, `RTRIM`,
`SECOND`, `SMALLINT`, `SQRT`, `STRIP`, `SUBSTR`/`SUBSTRING`, `TIME`,
`TIMESTAMP`, `TIMESTAMPDIFF`, `TRIM`, `TRUNC`/`TRUNCATE`, `UCASE`/
`UPPER`, `VARCHAR`, `WEEK`, `YEAR`, `ZONED`; aggregates `COUNT`,
`COUNT_BIG`, `SUM`, `AVG`, `MIN`, `MAX`.

Catalogs: `QSYS2.SYSSCHEMAS`, `SYSTABLES`, `SYSCOLUMNS`, `SYSINDEXES`,
`SYSKEYS`, `SYSVIEWS`, `SYSVIEWDEP`, `SYSCST`, `SYSCSTCOL`, `SYSKEYCST`,
`SYSREFCST`, `SYSTABLESTAT`, `SYSCOLUMNSTAT`, `SYSPARTITIONSTAT`.

Not supported, each refused with SQLCODE -84 and a message that says
so: `ALTER TABLE`, `CREATE ALIAS`, `CREATE TABLE AS`, common table
expressions (`WITH ... AS`), `UNIQUE` constraints (a `PRIMARY KEY`
becomes the file's unique key; for anything else create a unique
index), identity columns, expression and special-register column defaults, `FULL OUTER
JOIN`, a view over a join, a derived table, `VALUES` or `UNION`, a view
column that is an expression, a view condition DDS select/omit cannot
express, an index over a view, a join as the `MERGE` source, row
subqueries in `SET`, `SET SCHEMA` from a host variable, host variables
outside a program, long names without a system name, date arithmetic
without a labeled duration, `EXPLAIN` of anything but a query, and a
function the list above does not have or with the wrong number of
arguments. `OPNQRYF` is not in Phase 3: it maps onto the SQL executor
and is recorded as an open question.

## Service console

Everything is a console command with IBM-style `KEYWORD(value)`
parameters; `help` lists them. Text goes in through named line
buffers: `xfer put NAME N` stores the next `N` lines, and `srcput`,
`cpyfrmimpf`, `sqlrun` and `rla` read a buffer where the command would
read a stream file; `cpytoimpf` and `srcget` print theirs or store it
with `TOSTMF`. `sql` enters a line mode where `;` ends a statement and
`quit` leaves; `sql STATEMENT;` runs one. `rla NAME` runs a script of
record-level operations with the status, `%FOUND`/`%EQUAL`/`%EOF`
flags and field values each line expects, reporting every mismatch.
`dbbench N` inserts, reads by key and updates `N` rows of a five-field
keyed file with two logical files, under commitment control when the
job has started it. `chkstg` now also walks every keyed member and
compares its path with its records.

## The conformance kit

`tools/conformance/db/` is the fixed library `QSTEST` every database test
uses here and on the machine: ten physical files covering every field
type, every `DATFMT` and `TIMFMT`, nulls, `VARLEN`, `REUSEDLT` and a
source file; eight logical files covering key order and `DESCEND`,
multi-format paths, a join, static select/omit and `DYNSLT`; one CSV
per physical file; `rla/qstest.rla`, the record-level script with its
expected status codes; `sql/queries.sql`, twelve fixed queries.

`cargo test -p conformance --test db_kit` builds the library in memory
through `QDDSSRC` and `CRTPF`/`CRTLF`, loads the CSVs with
`CPYFRMIMPF`, journals `CUSTOMER` with both images, runs the script
(0 mismatches is the assertion), runs the queries, and writes every
output under `target/conformance-db/ours/`: `data/NAME.csv` for every
physical file and single-format logical file (key order), `ffd.csv`,
`lvlid.csv`, `rla.csv`, `rla.log`, `dspjrn.csv` and `sql/Qnn.csv`. It
then compares each with the same path under `tools/conformance/db/expected/`
and names the files that are not there. The same test regenerates the
oracle programs from the script and the queries and fails when they are
stale.

To produce the machine's outputs: copy `dds/` and `data/` to
`/home/QSTEST/`, `oracle/RLATEST.RPGLE` and `oracle/QRESULTS.SQL` to
`/home/QSTEST/oracle/`, compile and call `oracle/QSTEST.CLLE`. It
creates the library, loads the data, journals, runs the RPG, builds the
`QRESnn` tables and writes every file under `/home/QSTEST/out/`. Copy
that directory's contents into `expected/` and run the test again. The
comparer reads numbers by value, dates and times as digits, text
without trailing blanks, and the outfile-derived tables through the
`QADSPFFD`, `QAFDRFMT` and `QADSPJRN` column names; journal entries
compare on the `R` entries only. `rla.csv` from the machine carries
the real status codes and flags line by line; where they differ from
the script's `=>` column, the script is wrong and gets corrected.

Under QEMU, `build test` runs two more steps of the disk suite. Step 13
pushes the same DDS and CSVs over the console, creates and loads the
library, runs the script, and compares every export and query with the
in-memory run, then runs `dbbench` and `chkstg`. Step 14 updates the
journaled `CUSTOMER` under commitment control without committing, kills
the guest, and expects the next IPL to report `C1001082`, the balance
back to what it was, a `C RB` entry in the journal and clean access
paths.

## Numbers

The `dbbench` loop in memory on the development host (one keyed PF,
two logical files, batches of 1,000 rows per transaction, release
build; `cargo test --release -p conformance --test db_bench --
--ignored --nocapture`):

| Rows | Insert | Read by key | Update |
|---|---|---|---|
| 20,000 | 3498/s (5.7 s) | 11344/s (1.8 s) | 1458/s (13.7 s) |
| 200,000 | 2892/s (69.2 s) | 11823/s (16.9 s) | 1400/s (142.9 s) |

An insert maintains three access paths; an update reads for update,
rewrites the slot and maintains the three paths whose keys may have
changed, so it costs about twice an insert. The step-13 line under QEMU
with a virtio disk is the number to compare the machine with. The kit
test itself runs in under a second.

## Running

```
cargo test -p dds -p qdecimal -p db -p jrn -p sql   # the unit and integration tests
cargo test -p conformance --test db_kit             # QSTEST in memory, compared with expected/
QS400_UPDATE_ORACLE=1 cargo test -p conformance --test db_kit   # regenerate the oracle programs
cargo test --release -p conformance --test db_bench -- --ignored --nocapture
cargo run -p qdecimal --bin qdfuzz -- kit tools/conformance/oracle/qdecimal
cargo run -p build -- test                          # the disk suite with steps 13 and 14
```

## Known limits

- One job: locks, overrides, the commit definition and the SQL session
  are per caller-supplied job handle, with no call levels, activation
  groups or `OVRSCOPE`. Phase 5 made them per job, the overrides one
  table per job scoped by activation group.
- Writes, updates and deletes through a logical file were refused; they
  now go to the based-on member when every field of the format is a
  based-on field (`odp::lfwrite`).
- `WAITRCD` is kept but a locked record is not waited for. Since Phase
  5 the job waits, as `WAITRCD` says.
- A join logical file took the first matching secondary record only;
  it now gives one record per match (`member::join`), in `JDUPSEQ`
  order, honours `JDFTVAL`, searches a keyed secondary through its path
  and tests select/omit on secondary fields at read time.
- `ORDER BY` is an external merge sort (`sql::exec::sort`): rows past
  the job's `sort_budget` (100,000 by default) go to the spill area as
  sorted runs and are merged on the way out. The spill area
  (`sql::exec::spill`) is memory-backed, as the temporary ASP is on
  this system; `FileSpill` writes runs to a directory for a host whose
  temporary storage is a file system. Keyed scans read their paths in
  batches. Joins, grouping, projection and `DISTINCT` still hold their
  rows between steps, so a query without a sort is bounded by memory.
- `OPNQRYF`, `ALTER TABLE`, aliases, constraints other than unique
  indexes, identity columns, `FULL OUTER JOIN`, triggers, stored
  procedures and functions, `LOB`, `DBCS` and `GRAPHIC` data are out.
- Commitment control is scoped: `STRCMTCTL CMTSCOPE(*ACTGRP)` gives the
  activation group its own commit definition, `*JOB` the job's one that
  every group without its own reaches; a group's definition ends with
  the group. `OMTJRNE` beyond `*OPNCLO`, `MINENTDTA` and `FIXLENDTA` are
  kept in the attributes and not acted on.
- The console's `cpyfrmimpf` and `cpytoimpf` read and write UTF-8
  buffers; `STMFCODPAG` is not a parameter.
- `tools/conformance/db/expected/` is empty until the kit has run on a
  machine; the comparison is a skip, not a pass, until then.

## Acceptance evidence

On 2026-10-11, on the development host: `cargo test --workspace` green,
70 suites and 328 tests, including `db` (18 unit tests and the `cmt`, `dsp`,
`file`, `jrn`, `ovr`, `path`, `rla`, `spec`, `util` suites), `jrn`,
`sql` (`parse`, `exec`, `session`), `dds` (`pf`, `lf`) and the
conformance kit test with 0 script mismatches, 12 queries and 32 files
written; `build deny` clean on both Linux targets with 170 crates and
no C build helpers; clippy clean for the host workspace and for `slic`
and `console` on musl; `#![deny(missing_docs)]` on `dds`, `qdecimal`,
`db`, `jrn` and `sql`; `database/db/tests/spec.rs` holds `DB_FORMAT.md` and the
code's layouts together, including the journal structures.

The QEMU steps 13 and 14 are written and wired into the disk suite;
their lines belong here after the next `build test` run on the Linux
host, as do the machine's outputs under `expected/` after the kit has
run on an IBM i.

## Deviations from the Phase 3 text

- Views are select/omit logical files, so a view is a plain selection
  of one table with conditions of a column against a literal; anything
  else is refused with -84 rather than stored as a query.
- `UNIQUE` constraints in `CREATE TABLE` are refused; `PRIMARY KEY`
  becomes the physical file's `UNIQUE` key and any other unique set is
  a unique index, since a constraint object does not exist.
- The RLA script's `OPEN` names the usage (`INPUT`, `UPDATE`, `OUTPUT`,
  `ALL`) and whether the open is `KEYED`, since RPG declares both and
  `WRITE` to a file opened for update alone is not a runtime case.
- `CPYTOIMPF` and `CPYFRMIMPF` take `TIMFMT` as well as `DATFMT`, and a
  float field accepts exponent notation on import.
- The harness's step numbering continues from Phase 2 (13 and 14) and
  the crash step follows the kit step before the abnormal-IPL step, so
  the library it needs is on the working image.
