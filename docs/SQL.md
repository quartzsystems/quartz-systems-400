# QS/400 SQL: the Db2 for i dialect

QS/400 speaks the Db2 for i SQL dialect, 7.5 level, with the exceptions
this document lists. It is the single feature matrix of the SQL engine
(`database/sql`) and everything around it: the statements, clauses,
functions, types, catalogs and services, the tools, and what each was
checked against. It becomes the "Db2 for i compatibility" section of the
release notes, so a row says what the code does at the commit it is in.

**Status.** *supported*: implemented and, where a case is named, agreeing
with the oracle. *partial*: implemented with the limits given. *refused
SQLnnnn*: not implemented; the statement fails with that SQLCODE and a
message saying so (the honesty rule: nothing is accepted and ignored).
*no-op*: accepted, documented as without effect, as IBM's own behaviour
or a hint allows.

**Oracles.** *primary*: Cody's V7R1M0 system (Db2 for i 7.1), through the
host servers. *pub400*: pub400.com, V7R5M0, for 7.2 and later. *none*:
QS/400 behaviour with no oracle, with the reason.

**Tests.** `area/NAME` is a case of the SQL kit
(`tools/conformance/sql/cases/area/NAME.sql`, outcome in `expected/`);
`crate::file` a cargo test. `cargo test -p conformance --test sql_kit` runs
the kit; `tools/conformance/sql/known.txt` lists the cases that still
differ from their oracle.

The work that takes the engine from the Phase 3 subset to this matrix is
in progress; its plan is the SQL completion brief. Sections below mark
rows the work has not reached as refused, as the code does today.

## 1. Storage of SQL objects

| Item | Status | Notes |
|---|---|---|
| SQL attributes of any object (`*SQLINF`, the header's `sql` link) | supported | `docs/STORE_FORMAT.md` §8-9, `docs/DB_FORMAT.md` §8; store header version 2, version 1 headers read as having none |
| Library SQL name index | supported | `docs/DB_FORMAT.md` §8.2; schemas in QSYS's |
| `*SQLUDT` object type | supported (type only) | |
| `*SQLINF` entry bodies | draft | the `sql-info` format in `tools/fmtcheck` until the work freezes it |
| Save and restore of `*SQLINF` | refused | not yet a save part: a restored object comes back without its SQL attributes |

## 2. Statements

| Statement | Status | Oracle | Test |
|---|---|---|---|
| `SELECT`, `VALUES`, subselect, fullselect | supported (Phase 3 subset, §3) | primary | `kit/BASIC` |
| `INSERT` (values, multi-row, subselect) | supported | primary | `kit/BASIC` |
| `UPDATE`, `DELETE` (searched, positioned) | supported | primary | `kit/BASIC` |
| `MERGE` | partial: no joined source | primary | |
| `DECLARE CURSOR`, `OPEN`, `FETCH`, `CLOSE` | supported | primary | |
| `CREATE TABLE` | partial: see §4 | primary | `kit/BASIC` |
| `CREATE INDEX` | partial: see §4 | primary | |
| `CREATE VIEW` | partial: views are select/omit logical files | primary | |
| `CREATE ALIAS` | partial | primary | |
| `CREATE SCHEMA`, `CREATE COLLECTION` | partial: library with `QSQJRN` and `QSQJRN0001`; no `AUTHORIZATION` | primary | |
| `DROP TABLE/INDEX/VIEW/ALIAS` | supported | primary | |
| `DROP SCHEMA` | refused SQL0084 | primary | |
| `ALTER TABLE` | partial | primary | |
| `LABEL ON`, `COMMENT ON` | partial: tables, columns, indexes, aliases | primary | |
| `RENAME` | partial | primary | |
| `COMMIT`, `ROLLBACK` | supported | primary | |
| `SET SCHEMA`, `SET PATH`, `SET CURRENT DEGREE`, `SET TRANSACTION` | partial | primary | |
| `EXPLAIN` | partial: a query only | none: our own plan text | |
| `CREATE SEQUENCE`, `ALTER/DROP SEQUENCE` | refused SQL0084 | primary | |
| `CREATE PROCEDURE`, `CREATE FUNCTION`, `CREATE TRIGGER`, `CREATE VARIABLE`, `CREATE TYPE` | refused SQL0084 | primary | |
| `CALL` | refused SQL0084 | primary | |
| `GRANT`, `REVOKE` | refused SQL0084 | primary | |
| `LOCK TABLE` | refused SQL0084 | primary | |
| `PREPARE`, `EXECUTE`, `EXECUTE IMMEDIATE`, `DESCRIBE` (dynamic) | refused SQL0084 outside embedded SQL | primary | |
| `CONNECT`, `DISCONNECT`, `RELEASE`, `SET CONNECTION` | refused SQL0084 | primary | |
| `SAVEPOINT`, `RELEASE SAVEPOINT`, `ROLLBACK TO SAVEPOINT` | refused SQL0084 | primary | |
| `TRUNCATE` | refused SQL0084 | pub400 | |
| `REFRESH TABLE` | refused SQL0084 | primary | |
| `DECLARE GLOBAL TEMPORARY TABLE` | partial: no `ON COMMIT` | primary | |
| SQL PL compound statements | refused SQL0084 | primary | |

## 3. Queries

| Clause | Status | Oracle | Test |
|---|---|---|---|
| Joins `INNER`, `LEFT`, `RIGHT`, `EXCEPTION`, `CROSS` | supported | primary | |
| `FULL OUTER JOIN` | refused SQL0084 | primary | |
| `JOIN ... USING`, `LATERAL`, `TABLE(...)` | refused SQL0084 | primary | |
| Derived tables | supported | primary | |
| `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY`, `DISTINCT` | supported | primary | `kit/BASIC` |
| `GROUPING SETS`, `ROLLUP`, `CUBE` | refused SQL0084 | primary | |
| `UNION`, `EXCEPT`, `INTERSECT` | supported | primary | |
| `FETCH FIRST n ROWS ONLY`, `LIMIT` | supported | primary | |
| `OFFSET` | refused SQL0084 | pub400 | |
| Common table expressions, recursive | refused SQL0084 | primary | |
| OLAP specifications (`ROW_NUMBER`, `RANK`, ... `OVER`) | refused SQL0084 | primary | |
| Subqueries: scalar, `EXISTS`, `IN`, quantified | supported | primary | |
| `CASE`, `CAST`, `BETWEEN`, `LIKE`, `IS NULL` | supported | primary | |
| `LIKE ... ESCAPE`, `IS DISTINCT FROM`, row-value predicates | refused SQL0084 | primary | |
| Labeled durations | partial: no `MICROSECONDS`, `WEEKS` | primary | |
| Date and time subtraction without a labeled duration | refused SQL0084 | primary | |
| Isolation clauses `WITH NC/UR/CS/RS/RR` | supported | primary | |
| `USE AND KEEP ... LOCKS` | accepted and ignored: to be implemented or refused (honesty rule) | primary | |
| `FOR UPDATE OF`, `FOR READ ONLY`, `OPTIMIZE FOR n ROWS` | partial | primary | |
| `SELECT ... FROM FINAL TABLE` | refused SQL0084 | primary | |

## 4. DDL details

| Item | Status | Oracle | Test |
|---|---|---|---|
| Column types | see §7 | | |
| `PRIMARY KEY` | partial: the file's unique key, no constraint object | primary | |
| `UNIQUE`, `CHECK`, `FOREIGN KEY` constraints | refused SQL0084 | primary | |
| Identity columns | partial: `INTEGER` start and increment only | primary | |
| Sequences | refused SQL0084 | primary | |
| Column defaults: constants, `NULL`, special registers | partial | primary | |
| Expression defaults, generated columns, row change timestamp | refused SQL0084 | primary | |
| Long names: columns | supported to 128 characters | primary | |
| Long names: tables, views, indexes, schemas (`FOR SYSTEM NAME`) | refused SQL0084 without a system name | primary | |
| `CREATE TABLE ... AS`, `CREATE TABLE ... LIKE` | refused SQL0084 | primary | |
| `CREATE OR REPLACE` | refused SQL0084 | pub400 | |
| Views over joins, unions, derived tables, expressions | refused SQL0084 | primary | |
| `WITH CHECK OPTION` | refused SQL0084 | primary | |
| Index extensions (`INCLUDE`, expression keys, `WHERE`, `UNIQUE WHERE NOT NULL`) | refused SQL0084 | primary | |
| `CREATE ENCODED VECTOR INDEX` | refused SQL0084 | primary | |
| `DROP ... RESTRICT/CASCADE` | partial | primary | |
| CREATE TABLE in a library that is not a schema: `SQL7905` | not given | primary | `kit/BASIC` |

## 5. Functions

Scalar functions supported today (Phase 3): `ABS`/`ABSVAL`, `ADD_MONTHS`,
`BIGINT`, `BINARY`, `BOOLEAN`, `CEIL`/`CEILING`, `CHAR`,
`CHAR_LENGTH`/`CHARACTER_LENGTH`, `COALESCE`/`VALUE`/`IFNULL`, `CONCAT`,
`DATE`, `DAY`, `DAYNAME`, `DAYOFMONTH`, `DAYOFWEEK`, `DAYOFWEEK_ISO`,
`DAYOFYEAR`, `DAYS`, `DEC`/`DECIMAL`, `DECFLOAT`, `DIGITS`,
`DOUBLE`/`DOUBLE_PRECISION`, `FLOAT`, `FLOOR`, `HEX`, `HOUR`, `INT`/`INTEGER`,
`JULIAN_DAY`, `LAST_DAY`, `LCASE`/`LOWER`, `LEFT`, `LEN`/`LENGTH`, `LOCATE`,
`LTRIM`, `MAX`, `MICROSECOND`, `MIN`, `MINUTE`, `MOD`, `MONTH`, `MONTHNAME`,
`NEXT_DAY`, `NORMALIZE_DECFLOAT`, `NOW`, `NULLIF`, `OCTET_LENGTH`,
`POSITION`/`POSSTR`, `POW`/`POWER`, `REAL`, `REPLACE`, `RIGHT`, `ROUND`,
`RRN`, `RTRIM`, `SECOND`, `SMALLINT`, `SQRT`, `STRIP`, `SUBSTR`/`SUBSTRING`,
`TIME`, `TIMESTAMP`, `TIMESTAMPDIFF`, `TRIM`, `TRUNC`/`TRUNCATE`,
`UCASE`/`UPPER`, `VARBINARY`, `VARCHAR`, `WEEK`, `YEAR`, `ZONED`.
Aggregates: `COUNT`, `COUNT_BIG`, `SUM`, `AVG`, `MIN`, `MAX`. Any other
function is refused with SQL0084 (to become SQL0204 or SQL0440 as IBM
gives them).

Result types of expressions: refused detail. Today an expression's
result column describes as `VARCHAR(32767)`; IBM describes the real type
(`kit/BASIC`).

The 7.5 reference list (the brief, Section 2) is the target; rows are
added here as families land in `database/sql/src/exec/func/`.

## 6. Special registers and global variables

| Item | Status | Oracle | Test |
|---|---|---|---|
| `CURRENT DATE/TIME/TIMESTAMP`, `CURRENT SCHEMA`, `CURRENT PATH`, `USER`, `CURRENT USER` | supported | primary | |
| Other `CURRENT` registers | refused SQL0084 | primary | |
| `QSYS2.*` and `SYSIBM.*` built-in global variables | refused SQL0084 | pub400 | |

## 7. Data types

| Type | Status | Oracle | Test |
|---|---|---|---|
| `SMALLINT`, `INTEGER`, `BIGINT`, `DECIMAL`, `NUMERIC` | supported | primary | `kit/BASIC` |
| `REAL`, `DOUBLE`, `FLOAT` | supported | primary | `kit/BASIC` |
| `DECFLOAT` | partial | primary | |
| `CHAR`, `VARCHAR` | supported | primary | `kit/BASIC` |
| `GRAPHIC`, `VARGRAPHIC` | partial | primary | |
| `BINARY`, `VARBINARY` | partial | primary | |
| `CLOB`, `BLOB`, `DBCLOB` | partial | primary | |
| `DATE`, `TIME`, `TIMESTAMP` | supported | primary | `kit/BASIC` |
| `TIMESTAMP(p)` | partial | primary | |
| `BOOLEAN` | partial | pub400 | |
| `ROWID`, `XML`, `DATALINK` | refused SQL0084 | primary | |
| JSON functions | refused SQL0084 | pub400 | |

## 8. Catalogs

| View | Status | Oracle | Test |
|---|---|---|---|
| `QSYS2.SYSSCHEMAS`, `SYSTABLES`, `SYSCOLUMNS`, `SYSINDEXES`, `SYSKEYS`, `SYSVIEWS`, `SYSVIEWDEP`, `SYSCST`, `SYSCSTCOL`, `SYSKEYCST`, `SYSREFCST`, `SYSTABLESTAT`, `SYSCOLUMNSTAT`, `SYSPARTITIONSTAT` | partial: computed from the store | primary | |
| `SYSIBM.SYSDUMMY1` | supported | primary | |
| Other `QSYS2` catalogs, `SYSIBM` metadata views | not present: SQL0204 | primary | |

## 9. Services, tools and interfaces

| Item | Status | Oracle | Test |
|---|---|---|---|
| `QSYS2` services (views, table functions, procedures) | not present: SQL0204 | pub400 | |
| `STRSQL` | partial (Phase 8) | pub400 | |
| `RUNSQLSTM` | partial: `COMMIT`, `ERRLVL` | primary | |
| `RUNSQL` | not present | primary | |
| SQLRPGLE precompiler (`CRTSQLRPGI`) | partial (Phase 9) | pub400 | |
| DRDA server (JDBC, ACS) | partial (Phase 10) | primary | |
| DRDA client | not present | primary | |
| SQL CLI (`QSQCLI`) | not present | primary | |
| `STRDBMON`, `PRTSQLINF`, `DSPSQLPKG` | not present | primary | |
| Query/400 (`WRKQRY`, `RUNQRY`, product QS00QU1) | not in this work: a follow-on product | | |

## 10. Refused

Every form the engine refuses, with its SQLCODE, is in the tables above
until the work ends; this section then lists the remaining refusals
verbatim for the release notes.

## 11. Benchmarks

Recorded by `build bench` when Section 7 of the work lands.

## Acceptance

Filled in when the work ends.
