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
| Save and restore of `*SQLINF` | supported | save format 2, part 11; a saved library carries its schema name (`saverst/tests/sqlinfo.rs`) |

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
| `CREATE VIEW` | supported: any query the engine runs, kept as query text; views DDS can express stay logical files | primary | `ddl/VIEWQRY` |
| `CREATE OR REPLACE VIEW`, `CREATE OR REPLACE ALIAS` | supported; refused SQL0084 when other views depend on the view | primary | `ddl/VIEWQRY`, `ddl/ALIAS` |
| `CREATE OR REPLACE TABLE` (`ON REPLACE PRESERVE/DELETE ROWS`) | partial: refused SQL0084 when views or indexes depend on the table | pub400 | `ddl/TABLEAS` |
| `CREATE OR REPLACE INDEX` | refused SQL0104, as IBM | primary | `ddl/INDEXEXT` |
| `CREATE ALIAS` (member aliases, long names) | supported | primary | `ddl/ALIAS` |
| `CREATE SCHEMA`, `CREATE COLLECTION` | supported: library, `QSQJRN` and `QSQJRN0001`, tables journaled, long names; no `AUTHORIZATION` | none: no library may be made on either oracle (NEEDS-CODY) | `sql::objects` |
| `DROP TABLE/INDEX/VIEW/ALIAS` with `RESTRICT` (SQL0616) and `CASCADE` | supported | primary | `ddl/DROPRES` |
| `DROP SCHEMA [RESTRICT | CASCADE]` | supported | primary (SQL0204 only) | `ddl/DROPRES` |
| `ALTER TABLE` | partial | primary | |
| `LABEL ON`, `COMMENT ON` | partial: tables, columns, indexes, aliases | primary | |
| `RENAME TABLE/INDEX` (`TO SYSTEM NAME`, `FOR SYSTEM NAME`) | supported; refused SQL0084 for an object other views read | primary, pub400 | `ddl/RENAME`, `ddl/SYSNAME` |
| `COMMIT`, `ROLLBACK` | supported | primary | |
| `SET SCHEMA`, `SET PATH`, `SET CURRENT DEGREE`, `SET TRANSACTION` | partial | primary | |
| `EXPLAIN` | partial: a query only | none: our own plan text | |
| `CREATE SEQUENCE`, `ALTER/DROP SEQUENCE` | refused SQL0084 | primary | |
| `CREATE PROCEDURE`, `CREATE FUNCTION`, `CREATE TRIGGER`, `CREATE VARIABLE`, `CREATE TYPE` | refused SQL0084 | primary | |
| `CALL` | refused SQL0084 | primary | |
| `GRANT`, `REVOKE` on tables and views (`WITH GRANT OPTION`, SQL0569, SQL0557, SQL7027) | supported, mapped to object authorities as the primary does | primary | `ddl/GRANT` |
| `GRANT`, `REVOKE` on schemas, routines, sequences, variables, types; column privileges | refused SQL0084 | none | `sql::objects` |
| `LOCK TABLE` | refused SQL0084 | primary | |
| `PREPARE`, `EXECUTE`, `EXECUTE IMMEDIATE`, `DESCRIBE` (dynamic) | refused SQL0084 outside embedded SQL | primary | |
| `CONNECT`, `DISCONNECT`, `RELEASE`, `SET CONNECTION` | refused SQL0084 | primary | |
| `SAVEPOINT`, `RELEASE SAVEPOINT`, `ROLLBACK TO SAVEPOINT` | refused SQL0084 | primary | |
| `TRUNCATE` | refused SQL0084 | pub400 | |
| `REFRESH TABLE` | refused SQL0084 | primary | |
| `DECLARE GLOBAL TEMPORARY TABLE`; any SQL object in QTEMP | refused SQL0084: BLOCKED(sql), QTEMP is the temporary ASP and a statement runs in one ASP transaction | pub400 | `ddl/DGTT` |
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
| Long names: tables, views, indexes, aliases, schemas; IBM's generated system names; `FOR SYSTEM NAME`, `FOR COLUMN`, SQL7029 | supported | primary, pub400 | `ddl/LONGNAME`, `ddl/DELIMIT`, `ddl/SYSNAME` |
| Result columns of `SELECT *` | named by their SQL (long) names | primary | `ddl/LONGNAME` |
| `CREATE TABLE ... AS (query) WITH [NO] DATA`, `DEFINITION ONLY` | partial: an expression column is refused SQL0084 until expression types land | pub400 | `ddl/TABLEAS` |
| `CREATE TABLE ... LIKE`, `(LIKE t)` | supported; `INCLUDING IDENTITY`, `EXCLUDING COLUMN DEFAULTS`, `USING TYPE DEFAULTS` refused SQL0084 | pub400 | `ddl/TABLEAS` |
| Views over joins, unions, grouping, expressions, views | supported; expression columns show as VARCHAR(256) in the catalog until expression types land | primary | `ddl/VIEWQRY` |
| `WITH [LOCAL | CASCADED] CHECK OPTION` (SQL0161) | partial: on views that stay logical files; refused SQL0084 on others | primary | `ddl/VIEWCHK` |
| `CREATE INDEX ... WHERE` (sparse) | partial: conditions select/omit can express; others refused SQL0084 | primary | `ddl/INDEXEXT` |
| `UNIQUE WHERE NOT NULL`, descending keys | supported | primary | `ddl/INDEXEXT` |
| Expression (derived) index keys | refused SQL0084: BLOCKED(sql), access paths hold no key expressions | primary | `ddl/INDEXEXT` |
| `INCLUDE` on an ordinary index | refused SQL0104, as IBM | primary | `ddl/INDEXEXT` |
| `CREATE ENCODED VECTOR INDEX [INCLUDE]` | no-op as an EVI: made as an ordinary keyed index; `INCLUDE` kept for the catalog only | primary | `ddl/INDEXEXT` |
| CREATE TABLE in a library that is not a schema: warning SQL7905 | supported | primary, pub400 | `kit/BASIC` |
| `VOLATILE`, `IN` on CREATE TABLE; `PAGESIZE`, `UNIT` on CREATE INDEX | accepted without effect: to be refused or documented as no-ops (honesty rule) | primary | |
| DSPFD of a view kept as query text | partial: shows a physical file without members, not IBM's view display | primary | |

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

Result types: an expression's result column describes with its real
type and its values are fitted to it (§7). Some functions still describe
as `VARCHAR(32767)` until the query work registers their types.

The 7.5 reference list (the brief, Section 2) is the target; rows are
added here as families land in `database/sql/src/exec/func/`.

## 6. Special registers and global variables

| Item | Status | Oracle | Test |
|---|---|---|---|
| `CURRENT DATE/TIME/TIMESTAMP`, `CURRENT SCHEMA`, `CURRENT PATH`, `USER`, `CURRENT USER` | supported | primary | |
| Other `CURRENT` registers | refused SQL0084 | primary | |
| `QSYS2.*` and `SYSIBM.*` built-in global variables | refused SQL0084 | pub400 | |

## 7. Data types

| Type or rule | Status | Oracle | Test |
|---|---|---|---|
| `SMALLINT`, `INTEGER`, `BIGINT`, `DECIMAL`, `NUMERIC` | supported | primary | `kit/BASIC`, `types/FLOATFMT` |
| `REAL`, `DOUBLE`, `FLOAT`; their text in `CHAR`, `VARCHAR`, `CAST`, `\|\|` (17 digits, `1.5E0`) | supported | primary | `types/FLOATFMT` |
| `DECFLOAT(16)`, `DECFLOAT(34)`: values, specials, DPD encoding, arithmetic (NaN, SQL0802, SQ20365) | supported | primary | `types/DECFLT1`, `types/DECFLT2` |
| `CURRENT DECFLOAT ROUNDING MODE` and `SET` (7 modes) | supported; a mode from a host variable refused SQL0084 | primary | `types/DECFLT1` |
| `QUANTIZE`, `NORMALIZE_DECFLOAT`, `TOTALORDER`, `COMPARE_DECFLOAT`, `DECFLOAT()` | supported | primary | `types/DECFLT1` |
| `DECFLOAT_FORMAT` | partial: elements `9 0 . , $ S MI PR` | pub400 | `types/DECFLT3` |
| `DECFLOAT` result type mixed with other numerics | partial: right where the select list is typed statically; values carry no declared type | primary | `types/DECFLT1` |
| `SQL_DECFLOAT_WARNINGS` | refused: the 7.1 default is fixed until QAQQINI | primary | |
| Casting matrix (SQL0461, 0420, 0802, 0180, 0181, 0433, 0189, 0332, 0171) | supported | primary | `types/CASTSTR`, `types/DECFLT2` |
| Data mapping errors in the select list: NULL with warning +802/+420/+180/+20365 | supported | primary | `types/CASTSTR` |
| Assignment errors (SQL0404, 0406, 0408, 0420) | supported | primary | `types/CASTSTR` |
| Result types of constants, `CAST`, arithmetic, `\|\|`, `CASE`, `COALESCE`, `VALUES`, `UNION` | supported | primary | `types/*` |
| `CHAR`, `VARCHAR` | supported | primary | `kit/BASIC` |
| `GRAPHIC`, `VARGRAPHIC`, `DBCLOB` in CCSID 1200 and 13488; `NCHAR`, `NVARCHAR`, `NCLOB`; `CCSID` on columns and `CAST`; `FOR SBCS DATA` | supported | primary | `types/LOB1`, `types/CASTSTR` |
| `FOR MIXED DATA` | refused SQL0604 | primary | `types/LOB1` |
| `BINARY`, `VARBINARY` (binary vs character comparison SQL0401) | supported | primary | `types/CASTSTR` |
| `CLOB`, `BLOB`, `DBCLOB`: functions, comparisons, `ORDER BY`, `GROUP BY`, `DISTINCT` | supported | primary | `types/LOB1` |
| A LOB as an index key | refused SQL0350 | primary | `types/LOB1` |
| `CLOB ... FOR BIT DATA` | refused SQL0683 | primary | `types/LOB1` |
| `INLINE LENGTH` | no-op: values up to 24 bytes already sit in the record's LOB reference | pub400 | `types/DECFLT3` |
| LOB locators, file reference variables, `FREE LOCATOR` | refused: BLOCKED(sql), the engine's locator table exists; the statements come with dynamic and embedded SQL | none | |
| `DATE`, `TIME`, `TIMESTAMP`, `TIMESTAMP(p)` 0 to 12 | supported | primary | `kit/BASIC` |
| `BOOLEAN`: literals, `IS [NOT] TRUE/FALSE/UNKNOWN`, text casts; numeric casts SQL0461, arithmetic SQL0402 | supported | pub400 | `types/BOOL1` |
| `XMLPARSE(DOCUMENT)`, `XMLSERIALIZE` (parse error SQ20398) | supported | primary | `types/XML1` |
| `XML` as a column or `CAST` type, `XMLPARSE(CONTENT)`, `XMLQUERY`, `XMLTABLE`, `XMLEXISTS`, `XMLVALIDATE`, `XMLAGG` and the rest | refused SQL0084 | primary | `types/XML1` |
| `ROWID`, `DATALINK` | refused SQL0084 | primary | `types/LOB1` |
| `JSON_OBJECT`, `JSON_ARRAY` (incl. a fullselect), `FORMAT JSON/BSON`, `RETURNING`, `ABSENT/NULL ON NULL`, `WITH UNIQUE KEYS` | supported | pub400 | `types/JSON1` |
| `JSON_VALUE`, `JSON_QUERY`, `JSON_EXISTS`, `IS JSON`, lax and strict paths, `ON EMPTY`, `ON ERROR` | supported | pub400 | `types/JSON1` |
| SQL/JSON path filters, item methods, `**`, `@`, variables, `PASSING` | refused SQL0084 | pub400 | |
| `JSON_TO_BSON`, `BSON_TO_JSON` | supported, byte-exact | pub400 | `types/JSON1` |
| `JSON_ARRAYAGG`, `JSON_OBJECTAGG` | supported | pub400 | `types/JSON1` |
| `JSON_TABLE` | partial: the evaluator is done; its use in `FROM` waits on the query work merging | pub400 | `sql::table` |
| `JSON_UPDATE` | refused SQL0204, as pub400 (7.5 has none) | pub400 | `types/JSON1` |

## 8. Catalogs

| View | Status | Oracle | Test |
|---|---|---|---|
| `QSYS2.SYSSCHEMAS`, `SYSTABLES`, `SYSCOLUMNS`, `SYSINDEXES`, `SYSKEYS`, `SYSVIEWS`, `SYSVIEWDEP`, `SYSCST`, `SYSCSTCOL`, `SYSKEYCST`, `SYSREFCST`, `SYSTABLESTAT`, `SYSCOLUMNSTAT`, `SYSPARTITIONSTAT` | partial: computed from the store; SQL and system names; SYSVIEWS with the 7.1 columns | primary | `ddl/LONGNAME`, `ddl/VIEWQRY` |
| `QSYS2.SYSTABAUTH` | supported | pub400 | `sql::objects` |
| `QSYS2.SYSCOLAUTH` | present, always empty: BLOCKED(sql), the store keeps no column authorities | pub400 | `sql::objects` |
| Catalog views in a schema (`CREATE SCHEMA`) | partial: the QSYS2 views above only | none | |
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
