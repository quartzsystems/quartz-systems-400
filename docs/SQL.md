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
| `MERGE` (joined sources, every clause) | supported | pub400 | `query/CHANGES` |
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
| `DROP SCHEMA [RESTRICT \| CASCADE]` | supported | primary (SQL0204 only) | `ddl/DROPRES` |
| `ALTER TABLE` ADD/DROP/ALTER COLUMN (SET DATA TYPE, SET/DROP DEFAULT, SET/DROP NOT NULL, SET GENERATED, RESTART, DROP IDENTITY), ADD/DROP CONSTRAINT | partial: refused SQL0084 (BLOCKED(sql)) when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column | primary | `integrity/CSTALTER`, `integrity/IDENTITY` |
| `ALTER TABLE ... ADD/DROP PARTITION`, materialized query tables | refused SQL0084 | none | `sql::integrity` |
| `LABEL ON`, `COMMENT ON` | partial: tables, columns, indexes, aliases | primary | |
| `RENAME TABLE/INDEX` (`TO SYSTEM NAME`, `FOR SYSTEM NAME`) | supported; refused SQL0084 for an object other views read | primary, pub400 | `ddl/RENAME`, `ddl/SYSNAME` |
| `COMMIT`, `ROLLBACK` | supported | primary | |
| `SET SCHEMA`, `SET PATH`, `SET CURRENT ...`, `SET TRANSACTION` | supported (§6) | pub400 | `query/REGISTERS` |
| `EXPLAIN` (a Quartz statement; pub400 answers SQL0104) | partial: a query's plan text with `QQRID`, `QQRCOD`, `QVC13` per line; ACS Visual Explain (`QSYS.QQQDBVE`, undocumented) not provided | none | `sql::planner` |
| `CREATE SEQUENCE`, `ALTER SEQUENCE`, `DROP SEQUENCE`, `NEXT VALUE FOR`, `PREVIOUS VALUE FOR` | supported, IBM's cache blocks matched; `ALTER SEQUENCE AS` refused SQL0084; a failed statement gives back the value it used, where IBM keeps it used | primary | `integrity/SEQUENCE` |
| `GRANT` on sequences | refused SQL0084 | none | |
| `CREATE [OR REPLACE] PROCEDURE` with an SQL body, compiled through QIR to a `*PGM` | supported | primary | `sqlpl/*` |
| `BEGIN ATOMIC`, `UNDO` handlers (procedures, functions, AFTER triggers; in BEFORE triggers UNDO acts as EXIT) | supported on savepoints; also under `COMMIT(*NONE)` through the routine's own commit definition; SQL0775, SQL0780 | primary | `sqlpl/ATOMIC`, `sqlpl/ATOMIC2` |
| `CREATE PROCEDURE ... EXTERNAL` (RPGLE, CL; `PARAMETER STYLE SQL`, `GENERAL`, `GENERAL WITH NULLS`) | supported, also before the program exists (SQL7909); service program procedures up to 64 parameters in the call; `LANGUAGE C/COBOL/PLI/JAVA`, `DB2GENERAL`, `JAVA`, `DB2SQL` refused SQL0084 | primary | `sqlpl/EXTERN`, `qrpgrt::sqlpl` |
| SQL PL: compound statements, `DECLARE` (variables, conditions, cursors, handlers), `SET`, `IF`, `CASE`, `LOOP`, `WHILE`, `REPEAT`, `FOR`, `LEAVE`, `ITERATE`, `GOTO`, `RETURN`, `CALL`, `SIGNAL`, `RESIGNAL`, `GET DIAGNOSTICS`, cursors, `PREPARE`/`EXECUTE`/`EXECUTE IMMEDIATE`, labels, `SQLSTATE`/`SQLCODE` variables, `CONTINUE` and `EXIT` handlers | supported | primary | `sqlpl/*` |
| `GET STACKED DIAGNOSTICS` (one stacked area) | supported | primary | `sqlpl/STACKED` |
| SQL PL variables of `GRAPHIC`/`VARGRAPHIC`/`DBCLOB`, `BINARY`/`VARBINARY`/`BLOB`, `DECFLOAT`, `BOOLEAN` | supported | pub400 | `sqlpl/PLTYPES`, `sqlpl/PLBOOL` |
| Parameter `DEFAULT`, `CALL p(name => v)`, `DEFAULT` as an argument (SQL0469, SQ20483) | supported | pub400 | `sqlpl/PARMDFT` |
| `ALLOCATE`/`ASSOCIATE`/`DESCRIBE` in routines, `EXECUTE ... INTO`, cursors and handlers in functions, `AS LOCATOR`, `AUTONOMOUS` | refused SQL0084 | primary | |
| Cursors `WITH RETURN [TO CALLER \| TO CLIENT]`, `DYNAMIC RESULT SETS` (SQL0464), `SET RESULT SETS CURSOR` | supported | primary | `sqlpl/RESULT1`, `sqlpl/DYNSQL` |
| `CALL` from embedded SQL and SQL routines; unqualified through the path or library list | supported | primary | `sqlpl/NESTED` |
| `CALL` from the database host server (JTOpen, ACS): OUT/INOUT values, result sets one after another, parameter modes and names | supported | primary | `hostserver::call`, replay `database-callproc` |
| `CALL` from DRDA: result sets as IBM i sends them (`RSLSETRM`, `SQLRSLRD`, per set `OPNQRYRM`, `SQLCINRD`, `QRYDSC`), output values as `SQLDTARD` | supported | pub400, primary | `ddm::session` |
| `CALL` from QAP | partial: the first result set only; OUT/INOUT refused SQL0084 | none | `qsjob qapsql` |
| `CALL` from STRSQL | supported (no result sets shown, as STRSQL) | none | |
| `CALL` and trigger-firing changes from `RUNSQL`, `RUNSQLSTM` | supported from a job's command line and programs; from the service console and QAP still refused SQL0084 | none | `qrpgrt::runsql` |
| `PREPARE` of a `CALL` with markers no procedure names | SQL0204, as the primary | primary | `sql::embed` |
| `DESCRIBE INPUT` of a `CALL` (types, names, modes) | supported | primary | `sql::embed` |
| `CREATE FUNCTION`: SQL scalar, SQL table (`RETURNS TABLE`, through `TABLE(f(args))`), `SOURCE`, overloading, `SPECIFIC` | supported | primary | `sqlpl/FUNCS`, `sqlpl/TABFUNC` |
| `CREATE FUNCTION ... EXTERNAL` scalar, `PARAMETER STYLE SQL`, `SCRATCHPAD`, `FINAL CALL`, `DISALLOW PARALLEL` | supported: called in the job mid-statement | none | `qrpgrt::extfunc` |
| External table functions; `GENERAL` style external functions; `DBINFO`; external functions during an in-place scan or from the service console | refused SQL0084: BLOCKED(sql) | none | |
| `DROP [SPECIFIC] PROCEDURE/FUNCTION` with signature (SQL0476) | supported | primary | `sqlpl/OVERLOAD` |
| `ALTER PROCEDURE`, `ALTER FUNCTION` (`ALTER` options, `REPLACE` definition, `SPECIFIC`) | supported; the specific name is kept | primary | `sqlpl/ALTERRTN` |
| Array types `CREATE TYPE ... ARRAY[n]`; array variables, constructors, `a[i]`, `CARDINALITY`, `MAX_CARDINALITY`, `TRIM_ARRAY`, `ARRAY_EXISTS`, `ARRAY_AGG(... ORDER BY)`, `UNNEST ... WITH ORDINALITY` | supported in SQL procedures; elsewhere SQ20441; SQ20439, SQ20440, SQL0304 | primary | `sqlpl/ARRAYS`, `sqlpl/ARRAYERR` |
| Array parameters, array arguments in `CALL` | refused SQL0084: BLOCKED(sql) | primary | `sqlpl/ARRAYS` |
| `CREATE [OR REPLACE] VARIABLE` (`DEFAULT`, `CONSTANT`), `SET` (also the row form and `DEFAULT`), use in expressions and DML, `DROP VARIABLE` | supported; `SET` of a constant SQL0151 | primary | `objects2/VARIABLE` |
| `CREATE TYPE ... AS` source type `[WITH COMPARISONS]`, `DROP TYPE` | supported (a `*SQLUDT`) | primary | `sql::objects2` |
| Distinct-type columns (`CREATE TABLE`, `AS`, `LIKE`, views), `CAST` to and from them, generated cast functions, strong typing (SQL0401, 0402, 0171, 0131, 0581, 0415, 0408, 0432), `SYSCOLUMNS` `USER_DEFINED_TYPE_*`, DROP TYPE in use SQL0478 | supported; a comparison with an `IN`/`ANY` subquery's column is not type-checked; the SQLDA shows the source type | primary | `objects2/TYPES`, `TYPES2`, `TYPES3` |
| `WITH COMPARISONS` | accepted, no effect (as on the primary) | primary | `objects2/TYPES2` |
| Distinct types as SQL PL variables or parameters, sequence types, in `ALTER TABLE` | refused SQL0084: BLOCKED(sql) | primary | |
| `CREATE [OR REPLACE] TRIGGER` `BEFORE`/`AFTER`, `INSERT`/`UPDATE [OF]`/`DELETE`, `FOR EACH ROW`, `REFERENCING OLD/NEW [ROW]`, `WHEN`, `MODE DB2ROW/DB2SQL`, `SET OPTION`; `DROP TRIGGER`; `LABEL ON`/`COMMENT ON TRIGGER` | supported: a `*PGM` named as the primary names it, over the trigger buffer, so record I/O fires SQL triggers too; SQL7051 for a BEFORE trigger in DB2SQL mode | primary | `trigger/TRGFIRE`, `trigger/TRGDEFS` |
| `FOR EACH STATEMENT`, `OLD/NEW TABLE`, `INSTEAD OF`, several events (`INSERT OR ...`), `SECURED` | refused SQL0084 after IBM's own checks (SQL0628, SQL0696, SQL0159, SQL0156): BLOCKED(sql) | primary | `trigger/TRGDEFS` |
| `BEGIN ATOMIC` in a trigger body | supported | none | `qrpgrt::sqltrg` |
| `GRANT`/`REVOKE` `USAGE`/`ALTER`/`ALL` on types, `READ`/`WRITE`/`ALTER`/`ALL` on variables | supported, mapped to the authorities the primary shows | primary | `objects2/GRANTUDT` |
| `GRANT`, `REVOKE` on tables and views (`WITH GRANT OPTION`, SQL0569, SQL0557, SQL7027) | supported, mapped to object authorities as the primary does | primary | `ddl/GRANT` |
| `GRANT`, `REVOKE` on schemas, routines, sequences, variables, types; column privileges | refused SQL0084 | none | `sql::objects` |
| `LOCK TABLE` | refused SQL0084 | primary | |
| `PREPARE`, `EXECUTE`, `EXECUTE IMMEDIATE`, `DESCRIBE`, cursor statements, `WHENEVER`, `INCLUDE`, locator statements given to a dynamic interface (JDBC, STRSQL, RUNSQLSTM, `PREPARE` text) | refused as IBM refuses them: SQL0104, SQL0084, SQL0312 for host variables, SQL0199 | primary | `dyn/NOPREP` |
| SQLDA from `DESCRIBE OUTPUT/INPUT/TABLE` and `PREPARE INTO` (`USING NAMES/SYSTEM NAMES/LABELS/ANY/BOTH/ALL`, LOB extended entries, +237) | supported byte for byte, except the job CCSID and a reserved area IBM leaves unset | pub400 | `rpg/b11sql/SQL056`, `SQL057` |
| `DESCRIBE CURSOR`, `DESCRIBE PROCEDURE`, `ALLOCATE/ASSOCIATE LOCATORS` in programs | refused SQL0084 (requesters describe `RS#n` cursors internally) | none | |
| Scrollable cursors (`SCROLL`, every orientation, SQL0231), `WITH HOLD`, `INSENSITIVE`, `ASENSITIVE` | supported | pub400 | `rpg/b11sql/SQL058` |
| `SENSITIVE` cursors | partial: a plain single-table query, read again at each fetch; others SQL0243; JDBC `TYPE_SCROLL_SENSITIVE` is treated as insensitive | pub400 | `sql::embed` |
| `SET OPTION` (every program option, SQL5066 order) | supported; values QS/400 cannot honour refused SQL0084 | pub400 | `rpg/b11sql/SQL061`, `SQL062` |
| Blocked `INSERT ... n ROWS`, indicator arrays, `GET DIAGNOSTICS` in programs, `DECLARE TABLE`, `DECLARE STATEMENT` | supported | pub400 | `rpg/b11sql/SQL063`, `SQL064` |
| `SQLTYPE` host variables (LOBs, locators, `BINARY`, `VARBINARY`, `ROWID`, `RESULT_SET_LOCATOR`), `FREE LOCATOR`, `HOLD LOCATOR` (SQL7034, SQL0423) | supported | pub400 | `rpg/b11sql/SQL060`, `SQL065` |
| LOB file reference variables | declared; any use refused SQL0084: BLOCKED(sql), the job's runtime has no IFS access | pub400 | `rpg/b11sql/SQL060` |
| `CONNECT TO [USER ... USING ...]`, `CONNECT RESET`, `SET CONNECTION`, `RELEASE`, `DISCONNECT` (DRDA client, DUW) | partial: programs' embedded SQL and the job-side requesters; queries, cursors, immediate statements and `PREPARE`/`EXECUTE` run remotely; remote input values, scrolling `FETCH`, `DESCRIBE`, `GET DIAGNOSTICS` refused SQL0084; SQL0842, 0843, 0752, 0950, 30080, 30082 | primary (as the remote server) | `drdar::sessions`, `conformance::drda_client` |
| `RDBCNNMTH(*RUW)` | refused SQL0084 (accepted and ignored before) | none | `drdar` |
| Three-part names, `CREATE ALIAS` over a remote table | refused SQL0104 | none | |
| `SAVEPOINT [UNIQUE] ON ROLLBACK RETAIN CURSORS [ON ROLLBACK RETAIN LOCKS]`, `RELEASE [TO] SAVEPOINT`, `ROLLBACK TO SAVEPOINT` (SQL0880, 0881, 0882) | supported; without commitment control a savepoint is named and undoes nothing, as on the primary | primary | `sqlpl/SAVEPT` |
| `TRUNCATE` (all options) | supported; SQL0100 on an empty table; `RESTART IDENTITY` on an identity table refused SQL0084 (BLOCKED(sql), joins identity state) | pub400 | `query/CHANGES` |
| `REFRESH TABLE` | refused SQL0084 | primary | |
| `DECLARE GLOBAL TEMPORARY TABLE` (columns, `LIKE`, `AS ... WITH [NO] DATA`, `ON COMMIT DELETE/PRESERVE ROWS`, `NOT LOGGED [ON ROLLBACK ...]`, `WITH REPLACE`, `RCDFMT`) | supported in the job's SQL QTEMP (a temporary root library in ASP 1, dropped when the job ends); logged tables journaled to `QTEMP/QSQJRN`; rows cleared at SQL `COMMIT`/`ROLLBACK` only, not a CL or RPG commit; SQL0079, SQL0526, SQL0199 as IBM | pub400, primary | `ddl/DGTT`, `ddl/DGTTOPT`, `ddl/DGTTCLS`, `sql::qtemp` |
| `CREATE TABLE/VIEW/INDEX/ALIAS` in QTEMP | supported; sequences and variables in QTEMP SQL0707; types, routines and triggers there refused SQL0084; a permanent view over a QTEMP table SQL7008 | primary | `ddl/QTEMPOBJ` |
| One QTEMP per job, shared by CL, RPG, record I/O and SQL (a temporary root library in ASP 1, made at the job's start, dropped at its end) | supported | primary | `ddl/QTEMPCL`, `sql::qtemp_job` |

## 3. Queries

| Clause | Status | Oracle | Test |
|---|---|---|---|
| Joins `INNER`, `LEFT`, `RIGHT`, `EXCEPTION`, `CROSS`, `FULL OUTER` | supported | pub400 | `query/JOINS` |
| `JOIN ... USING` | supported; `ORDER BY` an unqualified USING column resolves here where pub400 gives SQL0203 | pub400 | `query/JOINS` |
| `LATERAL`, `TABLE(fullselect)`, `TABLE(f(args)) AS x` | supported; a RIGHT or FULL lateral join, and a lateral or USING join as a MERGE source, refused SQL0084 | pub400 | `query/JOINS` |
| Derived tables and nested table expressions anywhere; `ORDER BY`, `FETCH FIRST` in subselects | supported | pub400 | `query/LIMITS` |
| `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY`, `DISTINCT` | supported | primary | `kit/BASIC` |
| `GROUPING SETS`, `ROLLUP`, `CUBE`, `GROUPING()` | supported | pub400 | `query/GROUPSETS` |
| `UNION`, `EXCEPT`, `INTERSECT` | supported | primary | |
| `FETCH FIRST n ROWS ONLY`, `LIMIT`, `OFFSET`, `LIMIT n OFFSET m`, `LIMIT m,n` | supported | pub400 | `query/LIMITS` |
| `FETCH FIRST n PERCENT` | refused SQL0104, as pub400 | pub400 | `query/LIMITS` |
| `ORDER BY` column number (SQL0125 past the list), `ORDER OF` | supported | pub400 | `query/LIMITS` |
| `NULLS FIRST`, `NULLS LAST` | refused SQL0199, as pub400 | pub400 | `query/LIMITS` |
| Common table expressions, chained and recursive, `CYCLE` | supported; `CYCLE ... USING` refused SQL0084 | pub400 | `query/CTE` |
| `SEARCH DEPTH/BREADTH FIRST ... SET` | partial: the `SET` column can be selected here, pub400 gives SQL0901 | pub400 | `query/CTE` |
| Recursion without end | ends at 100,000 levels or the memory limit with SQL0904; `FETCH FIRST` does not stop it; no SQL0347 (pub400 shows none over JDBC) | pub400 | `query/CTE` |
| OLAP: `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `NTILE`, `PERCENT_RANK`, `CUME_DIST`, `LAG`/`LEAD` with `RESPECT/IGNORE NULLS`, `FIRST_VALUE`/`LAST_VALUE`/`NTH_VALUE`, `RATIO_TO_REPORT`, aggregates `OVER` with `ROWS` and `RANGE` frames | supported; `NTH_VALUE ... FROM LAST` refused SQL0084 | pub400 | `query/OLAP` |
| Subqueries: scalar, `EXISTS` at any depth, `IN`, quantified | supported | pub400 | `query/PREDS` |
| `IN` with row values, row `=` and `<>`, `IS [NOT] DISTINCT FROM`, `LIKE ... ESCAPE`, `BETWEEN` with expressions | supported; row `<`/`>` SQL0115 as IBM | pub400 | `query/PREDS` |
| `LIKE_REGEX` | refused SQL0104, as pub400 | pub400 | `query/PREDS` |
| `ARRAY_EXISTS`, `XMLEXISTS` | refused SQL0084 | none | |
| `CASE`, `CAST`, `IS NULL` | supported | primary | |
| Date and time arithmetic: date - date, time - time, timestamp - timestamp, durations `DECIMAL(8,0)`, `(6,0)`, `(14+s,s)`, labeled durations, `TIMESTAMP(p)` to 12 | supported; date + integer SQL0182, a unit on the wrong type SQL0187, `WEEKS` SQL0104 (pub400 has none) | pub400 | `query/DATEARITH` |
| Isolation clauses `WITH NC/UR/CS/RS/RR` | supported | primary | |
| `SKIP LOCKED DATA` on queries | supported; on other statements refused SQL0084 | pub400 | |
| `USE AND KEEP ... LOCKS`, `WAIT FOR OUTCOME`, `USE CURRENTLY COMMITTED` | refused SQL0084 | pub400 | |
| `FOR UPDATE OF`, `FOR READ ONLY` | supported | pub400 | `query/LIMITS` |
| `OPTIMIZE FOR n ROWS`, `ALL ROWS` | supported: a row goal for join and index choice; `0 ROWS` SQL0405, as pub400 | pub400 | `perf/OPTFOR` |
| `FINAL TABLE`, `NEW TABLE` of `INSERT` | supported; of `UPDATE/DELETE/MERGE` SQL0199, as pub400 | pub400 | `query/CHANGES` |
| `MERGE` with a joined source, `DELETE` with a correlated subquery | supported | pub400 | `query/CHANGES` |
| `UPDATE ... FROM` | refused SQL0199, as pub400 | pub400 | `query/CHANGES` |

## 4. DDL details

| Item | Status | Oracle | Test |
|---|---|---|---|
| Column types | see §7 | | |
| `PRIMARY KEY`, `UNIQUE` (table and column level), IBM's generated names | supported; tables are arrival sequence, keys are constraints | primary | `integrity/CSTBASIC` |
| `CHECK` | supported: comparisons, `AND/OR/NOT`, `IS NULL`, `BETWEEN`, `IN`, `LIKE`, arithmetic, concatenation, `UPPER LOWER LENGTH SUBSTR TRIM LTRIM RTRIM ABS MOD COALESCE`; other functions refused SQL0084 | primary | `integrity/CSTALTER` |
| `FOREIGN KEY`: `ON DELETE NO ACTION/RESTRICT/CASCADE/SET NULL/SET DEFAULT`, `ON UPDATE NO ACTION/RESTRICT` | supported; `ON UPDATE CASCADE` SQL0104 as IBM; cascaded rows take no record locks yet | primary | `integrity/CSTREF` |
| Constraints enforced for RPG, RLA and SQL writes alike (CPF502D, CPF502F, CPF503A, CPF523B, RPG status 01022) | supported | primary | `db::cst` |
| Check pending | partial: an open data path refuses it (SQL0971, CPF523B); SQL reads that open none do not | primary | `db::cst` |
| `NOT ENFORCED`, `DEFERRABLE`, `ENABLE/DISABLE QUERY OPTIMIZATION` | refused SQL0084 | none | |
| Identity columns: `START`, `INCREMENT`, `MINVALUE/MAXVALUE`, `CYCLE`, `CACHE`, `ORDER`, `BIGINT`/`DECIMAL`, `OVERRIDING SYSTEM/USER VALUE`, `IDENTITY_VAL_LOCAL()` | partial: a range past 19 digits refused SQL0084 (BLOCKED(sql)); `ORDER` kept, without effect on one system | primary | `integrity/IDENTITY` |
| Column defaults: constants, `NULL`, special registers | partial | primary | |
| `ROW CHANGE TIMESTAMP` columns | supported | primary | `integrity/ROWCHG` |
| Audit columns (`DATA CHANGE OPERATION`, `SESSION_USER`, ...) | supported, with pub400's type rules | pub400 | `integrity/AUDITCOL` |
| `GENERATED ALWAYS AS (expression)` | SQL0104, as IBM gives | primary, pub400 | `integrity/ROWCHG` |
| `IMPLICITLY HIDDEN` | refused SQL0084: BLOCKED(sql), queries have no hidden-column flag yet (IBM accepts it) | pub400 | `integrity/AUDITCOL` |
| Long names: columns | supported to 128 characters | primary | |
| Long names: tables, views, indexes, aliases, schemas; IBM's generated system names; `FOR SYSTEM NAME`, `FOR COLUMN`, SQL7029 | supported | primary, pub400 | `ddl/LONGNAME`, `ddl/DELIMIT`, `ddl/SYSNAME` |
| Result columns of `SELECT *` | named by their SQL (long) names | primary | `ddl/LONGNAME` |
| `CREATE TABLE ... AS (query) WITH [NO] DATA`, `DEFINITION ONLY` | partial: an expression column is refused SQL0084 until expression types land | pub400 | `ddl/TABLEAS` |
| `CREATE TABLE ... LIKE`, `(LIKE t)` | supported; `INCLUDING IDENTITY`, `EXCLUDING COLUMN DEFAULTS`, `USING TYPE DEFAULTS` refused SQL0084 | pub400 | `ddl/TABLEAS` |
| Views over joins, unions, grouping, expressions, views | supported; expression columns show as VARCHAR(256) in the catalog until expression types land | primary | `ddl/VIEWQRY` |
| `WITH [LOCAL \| CASCADED] CHECK OPTION` (SQL0161) | partial: on views that stay logical files; refused SQL0084 on others | primary | `ddl/VIEWCHK` |
| `CREATE INDEX ... WHERE` (sparse) | partial: conditions select/omit can express; others refused SQL0084 | primary | `ddl/INDEXEXT` |
| `UNIQUE WHERE NOT NULL`, descending keys | supported | primary | `ddl/INDEXEXT` |
| Expression (derived) index keys | refused SQL0084: BLOCKED(sql), access paths hold no key expressions | primary | `ddl/INDEXEXT` |
| `INCLUDE` on an ordinary index | refused SQL0104, as IBM | primary | `ddl/INDEXEXT` |
| `CREATE ENCODED VECTOR INDEX [INCLUDE]` | no-op as an EVI: made as an ordinary keyed index; `INCLUDE` kept for the catalog only | primary | `ddl/INDEXEXT` |
| CREATE TABLE in a library that is not a schema: warning SQL7905 | supported | primary, pub400 | `kit/BASIC` |
| `VOLATILE`, `IN` on CREATE TABLE; `PAGESIZE`, `UNIT` on CREATE INDEX | accepted without effect: to be refused or documented as no-ops (honesty rule) | primary | |
| DSPFD of a view kept as query text | partial: shows a physical file without members, not IBM's view display | primary | |

## 5. Functions

Scalar functions are families in `database/sql/src/exec/func/`; result
types are in `exec/restype/`. Functions IBM does not have answer SQL0204;
a known function with the wrong arguments SQL0170 or SQL0440.

| Functions | Status | Oracle | Test |
|---|---|---|---|
| Phase 3 set: `ABS`, `ADD_MONTHS`, `BIGINT`, `CEIL`, `CHAR`, `COALESCE`, `CONCAT`, `DATE`, `DAY`..., `DEC`, `DIGITS`, `DOUBLE`, `FLOOR`, `HEX`, `INT`, `LCASE`, `LEFT`, `LENGTH`, `LOCATE`, `LTRIM`, `MOD`, `NULLIF`, `POSSTR`, `POWER`, `REPLACE`, `RIGHT`, `ROUND`, `RRN`, `SUBSTR`, `TIME`, `TIMESTAMP`, `TRIM`, `TRUNC`, `UCASE`, `VARCHAR`, `ZONED` and the rest of PHASE3's list | supported | primary | `kit/BASIC` |
| Strings: `TRANSLATE`, `OVERLAY`, `INSERT`, `REPEAT`, `SPACE`, `LPAD`, `RPAD`, `INSTR`, `LOCATE_IN_STRING`, `ASCII`, `CHR`, `SOUNDEX`, `DIFFERENCE`, `STRIP`/`LTRIM` with characters, `GREATEST`, `LEAST`, `NVL`, `DECODE`, `CHAR` with `ISO/USA/EUR/JIS/LOCAL` | supported (`DECODE` though pub400 answers SQL0204: the brief asks for it) | pub400 | `query/STRFUNCS` |
| `REGEXP_COUNT`, `REGEXP_INSTR`, `REGEXP_SUBSTR`, `REGEXP_REPLACE`; `REGEXP_LIKE` as a predicate | supported | pub400 | `query/REGEXP` |
| Dates: `TIMESTAMP_FORMAT`/`TO_DATE`, `VARCHAR_FORMAT`/`TO_CHAR`, `EXTRACT`, `TIMESTAMPDIFF`, `TIMESTAMPDIFF_BIG`, `TIMESTAMP_ISO`, `ROUND_TIMESTAMP`, `TRUNC_TIMESTAMP`, `WEEK_ISO`, `DAYOFWEEK_ISO`, `QUARTER`, `MIDNIGHT_SECONDS`, `MONTHS_BETWEEN`, `ADD_DAYS/HOURS/MINUTES/SECONDS/YEARS` | supported | pub400 | `query/DTFUNCS` |
| Numbers and bits: `EXP`, `LN`, `LOG10`, trigonometric and hyperbolic functions, `SIGN`, `PI`, `RADIANS`, `DEGREES`, `BITAND`, `BITOR`, `BITXOR`, `BITNOT`, `MULTIPLY_ALT` | supported | pub400 | `query/NUMFUNCS` |
| `HEX`, `HASH_MD5`, `HASH_SHA1`, `HASH_SHA256`, `HASH_SHA512`, `HASH`, `INTERPRET`, `HEXTORAW`, `VARCHAR_BIT_FORMAT`, `VARCHAR_FORMAT_BINARY` | supported, byte-exact | pub400 | `query/NUMFUNCS` |
| `RAND`, `GENERATE_UNIQUE` | partial: RAND's sequence is not IBM's; GENERATE_UNIQUE is 13 bytes and rising but not IBM's layout | none | |
| `SYSTOOLS.SPLIT` | supported | pub400 | `query/JOINS` |
| `ENCRYPT_AES`, `ENCRYPT_RC2`, `ENCRYPT_TDES`, `DECRYPT_*`, `GETHINT` | refused SQL0084: IBM's encrypted layout is unpublished, so nothing made here could be read there | pub400 | |
| `VERIFY_GROUP_FOR_USER` | refused SQL0084 | pub400 | |
| `TRIM_ARRAY`, `CARDINALITY`, `MAX_CARDINALITY` | supported in SQL procedures (§2) | primary | `sqlpl/ARRAYS` |
| `ASINH`, `ACOSH`, `DATE_TRUNC`, `DATE_PART`, `DAYS_BETWEEN` and the other `_BETWEEN`, `INITCAP`, `LEN`, `TO_MULTI_BYTE`, `RAWTOHEX`, `EBCDIC_CHR` | SQL0204, as pub400 answers | pub400 | |
| Aggregates `COUNT`, `COUNT_BIG`, `SUM`, `AVG`, `MIN`, `MAX`, with `DISTINCT` everywhere | supported, typed as IBM (AVG of DEC(9,2) is scale 24) | primary | `kit/BASIC` |
| `STDDEV`, `STDDEV_SAMP`, `STDDEV_POP`, `VARIANCE`, `VAR`, `VAR_SAMP`, `VAR_POP`, `COVARIANCE`, `COVARIANCE_SAMP`, `CORRELATION`, `REGR_*` | supported, to the last digit | pub400 | `query/STATAGG` |
| `MEDIAN`, `PERCENTILE_CONT`, `PERCENTILE_DISC`, `LISTAGG` (`WITHIN GROUP`, `ON OVERFLOW`, SQL0137) | supported | pub400 | `query/ORDAGG` |
| `ARRAY_AGG` | supported in SQL procedures; elsewhere SQ20441, as pub400 | primary, pub400 | `sqlpl/ARRAYS` |
| `XMLAGG` | refused SQL0084 | pub400 | |
| Aggregate or OLAP function where none may be | SQL0120 | pub400 | |

## 6. Special registers and global variables

| Item | Status | Oracle | Test |
|---|---|---|---|
| `CURRENT DATE/TIME/TIMESTAMP`, `CURRENT SCHEMA`, `CURRENT PATH`, `USER`, `CURRENT USER`, `SESSION_USER`, `SYSTEM_USER` | supported | pub400 | `query/REGISTERS` |
| `CURRENT SERVER`, `CURRENT TIMEZONE`, `CURRENT DEGREE`, `CURRENT DECFLOAT ROUNDING MODE`, `CURRENT IMPLICIT XMLPARSE OPTION`, `CURRENT CLIENT_*`; `SET` of `DEGREE`, `DECFLOAT ROUNDING MODE`, `IMPLICIT XMLPARSE OPTION`, `PATH` | supported; `CURRENT TIMEZONE` is 0 until the job's offset is set | pub400 | `query/REGISTERS` |
| `QSYS2.JOB_NAME`, `QSYS2.PROCESS_ID`, `QSYS2.SERVER_MODE_JOB_NAME`, `QSYS2.THREAD_ID`, `SYSIBM.CLIENT_*` | partial: null until the job fills them (wired with the job's SQL session) | pub400 | `query/REGISTERS` |

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
| `SQL_DECFLOAT_WARNINGS` | settable through `OVERRIDE_QAQQINI` at `*DEFAULT` only | primary | `sql::services` |
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
| `JSON_ARRAYAGG`, `JSON_OBJECTAGG` (also over windows) | supported | pub400 | `types/JSON1` |
| `JSON_TABLE` (in `FROM`, as a table function) | supported | pub400 | `types/JSON1` |
| `JSON_UPDATE` | refused SQL0204, as pub400 (7.5 has none) | pub400 | `types/JSON1` |

## 8. Catalogs

| View | Status | Oracle | Test |
|---|---|---|---|
| `QSYS2.SYSSCHEMAS`, `SYSTABLES`, `SYSCOLUMNS`, `SYSINDEXES`, `SYSKEYS`, `SYSVIEWS`, `SYSVIEWDEP`, `SYSTABLESTAT`, `SYSCOLUMNSTAT`, `SYSPARTITIONSTAT` | partial: computed from the store; SQL and system names; SYSVIEWS with the 7.1 columns | primary | `ddl/LONGNAME`, `ddl/VIEWQRY` |
| `QSYS2.SYSCST`, `SYSCSTCOL`, `SYSKEYCST`, `SYSCHKCST`, `SYSREFCST`, `SYSCSTDEP`, `SYSSEQUENCES` | supported with the oracle's columns | primary | `integrity/CSTBASIC`, `integrity/SEQUENCE` |
| `QSYS2.SYSROUTINES`, `SYSPROCS`, `SYSFUNCS`, `SYSPARMS`, `SYSROUTINEDEP` | supported (`PARSE_TREE`, `PARM_ARRAY` empty) | primary | `sqlpl/CATALOG` |
| `QSYS2.SYSVARIABLES`, `SYSTYPES`, `SYSTRIGGERS`, `SYSTRIGDEP`, `SYSTRIGUPD` | supported; LOB catalog columns are VARGRAPHIC where IBM has NCLOB; `SYSTRIGGERS.SQL_PATH` null | primary | `objects2/VARIABLE`, `trigger/TRGCAT` |
| `QSYS2.SYSTRIGCOL` | partial: transition variables and INSERT target columns; other tables' columns not listed | primary | `trigger/TRGCAT` |
| `QSYS2.SYSVARIABLEDEP`, `SYSJARCONTENTS`, `SYSJAROBJECTS` | present, empty | primary (columns) | `objects2/TYPES` |
| `QSYS2.SYSTABAUTH` | supported | pub400 | `sql::objects` |
| `QSYS2.SYSCOLAUTH` | present, always empty: BLOCKED(sql), the store keeps no column authorities | pub400 | `sql::objects` |
| Catalog views in a schema (`CREATE SCHEMA`) | partial: the QSYS2 views above only | none | |
| `SYSIBM.SYSDUMMY1` | supported | primary | |
| Other `QSYS2` catalogs, `SYSIBM` metadata views | not present: SQL0204 | primary | |

## 9. Services, tools and interfaces

| Item | Status | Oracle | Test |
|---|---|---|---|
| `QSYS2` service shapes (column names, types, order) | captured from pub400 for every listed service; a test holds the code to the capture | pub400 | `sql::service_shapes` |
| `LIBRARY_LIST_INFO`, `SYSTEM_VALUE_INFO` | supported | pub400 | `sql::services` |
| `SYSTEM_STATUS_INFO` (and `_BASIC`) | partial: no partition, segment, address-rate or machine-type figures (null) | pub400 | `sql::services` |
| `ACTIVE_JOB_INFO` | partial: SQL, lock-wait and client columns null | pub400 | `sql::services` |
| `JOB_INFO` | partial: active jobs only | pub400 | `sql::services` |
| `OBJECT_STATISTICS` | partial: journaling and source columns null | pub400 | `sql::services` |
| `USER_INFO` (and `_BASIC`), `GROUP_PROFILE_ENTRIES` | supported; three columns IBM declares NOT NULL are null | pub400 | `sql::services` |
| `OUTPUT_QUEUE_ENTRIES` (and `_BASIC`), `OUTPUT_QUEUE_INFO`, `SYSTOOLS.SPOOLED_FILE_DATA` | partial: no AFP or font columns | pub400 | `sql::services` |
| `JOBLOG_INFO` | partial: no message tokens; the message key is the position in the log | pub400 | `sql::services` |
| `HISTORY_LOG_INFO` | partial: `GENERATE_SYSLOG` and `EOF_DELAY` refused SQL0443 | pub400 | `sql::services` |
| `NETSTAT_INFO`, `NETSTAT_JOB_INFO`, `NETSTAT_INTERFACE_INFO`, `NETSTAT_ROUTE_INFO` | partial: socket options and sequence counters null | pub400 | `sql::services` |
| `SOFTWARE_PRODUCT_INFO`, `PTF_INFO`, `JOB_DESCRIPTION_INFO` | supported (no registration columns, no job switches) | pub400 | `sql::services` |
| `IFS_OBJECT_STATISTICS`, `IFS_READ`, `IFS_READ_UTF8`, `IFS_READ_BINARY` | supported | pub400 | `sql::services` |
| `CALL QSYS2.QCMDEXC` (both forms), `IFS_WRITE`, `IFS_WRITE_UTF8`, `IFS_WRITE_BINARY` | supported: qualified, or unqualified when no other procedure has the name; named arguments | pub400 | `sql::services` |
| `JOB_QUEUE_INFO`, `SUBSYSTEM_INFO`, `SUBSYSTEM_POOL_INFO` | partial: `CONTROLLING_SUBSYSTEM` always NO | pub400 | `sql::services` |
| `MESSAGE_QUEUE_INFO`, `REPLY_LIST_INFO`, `SCHEDULED_JOB_INFO`, `AUTHORIZATION_LIST_INFO`, `AUTHORIZATION_LIST_USER_INFO` | supported (no message tokens) | pub400 | `sql::services` (shape only) |
| `OBJECT_PRIVILEGES`, `OBJECT_OWNERSHIP` | supported | pub400 | `sql::services` |
| `USER_STORAGE` | partial: profile-entry counts null | pub400 | `sql::services` |
| `JOURNAL_INFO`, `JOURNAL_RECEIVER_INFO`, `JOURNALED_OBJECTS` | partial: no remote journals, no journal message queue | pub400 | `sql::services` |
| `SPOOLED_FILE_INFO`, `SYSTEM_STATUS` (table function) | partial: timestamp filters refused SQL0443; `RESET_STATISTICS` not applied | pub400 | `sql::services` |
| `LIBRARY_INFO` | partial: `LIBRARY_TYPE` and journaling columns null | pub400 | `sql::services` |
| Scalar `QSYS2.QCMDEXC` | partial: only commands that run in the storage service (CPF0006 for the others) | pub400 | `sql::services` |
| `BASE64_ENCODE/DECODE`, `URL_ENCODE/DECODE` | supported, as pub400 answers | pub400 | `sql::services` |
| `HTTP_GET/POST/PUT/PATCH/DELETE`, `_BLOB` and `_VERBOSE` forms | supported over `net/httpc` with TLS; a call holds the statement until it answers (at most 300 s) | none | `sql::services`, `httpc` |
| Live-state services outside a query or execute (e.g. the service console's `sql` mode) | SQL0443: they read the system only on a statement's fiber | none | |
| `MESSAGE_FILE_DATA`, `SECURITY_INFO` | partial: creation and level columns, pending levels and service-tools lock columns null | pub400 (shape) | `sql::services` |
| `DISPLAY_JOURNAL` | partial: `GENERATE_SYSLOG`, `EOF_DELAY` refused SQL0443; thread, remote and syslog columns null | pub400 (shape) | `sql::services` |
| `SYSTOOLS.AUDIT_JOURNAL_xx` (47 entry types) | partial: common columns and about 20 entry-specific columns; the rest null | pub400 (shape) | `sql::services` |
| `FUNCTION_INFO`, `FUNCTION_USAGE`, `LICENSE_INFO`, `SYSLIMTBL`, `SYSLIMITS`, `ELECTRONIC_SERVICE_AGENT_INFO`, `ASP_VARY_INFO`, `SERVER_SBS_ROUTING` | supported, no rows: QS/400 has none of these | pub400 (shape) | `sql::services` |
| `OBJECT_LOCK_INFO`, `MEMORY_POOL_INFO`, `MEMORY_POOL`, `SYSTEM_ACTIVITY_INFO`, `ASP_INFO`, `SYSDISKSTAT`, `HARDWARE_RESOURCE_INFO`, `MEDIA_LIBRARY_INFO`, `COLLECTION_SERVICES_INFO`, `EXIT_POINT_INFO`, `EXIT_PROGRAM_INFO`, `ENVIRONMENT_VARIABLE_INFO`, `GROUP_PTF_INFO`, `SYSTOOLS.GROUP_PTF_DETAILS`, `TCPIP_INFO`, `IFS_OBJECT_PRIVILEGES`, `IFS_OBJECT_LOCK_INFO` | partial: columns QS/400 keeps no value for are null (listed per service in `services/`); `RESET_STATISTICS => 'YES'` refused SQL0443; job-level environment variables not shown | pub400 (shape) | `sql::services` |
| `DNS_LOOKUP` | partial: the system resolver; `DOMAIN_SERVER` refused SQL0084 | pub400 (shape) | `sql::services` |
| `SYSTOOLS.CHANGE_USER_PROFILE` | partial: preview or run; special-authority actions other than `*SAME` refused SQL0084 | pub400 (shape) | `sql::services` |
| `OVERRIDE_TABLE` | no-op, checked: QS/400 does not block records | none | `sql::services` |
| `GENERATE_SQL`, `GENERATE_SQL_OBJECTS` | partial: tables, views, indexes, sequences, aliases with constraints, labels, grants and the drop, replace, unformatted, unqualified and system-name forms, laid out as pub400; other object types and options refused SQL0084; no member written in QTEMP | pub400 | `services/GENSQL` |
| `OVERRIDE_QAQQINI` | partial: tuning options kept as no-ops; options that change results or messages refused SQL0084 unless `*DEFAULT` | none | `sql::services` |
| `SET_SERVER_SBS_ROUTING`, `SET_PASE_SHELL_INFO` (SSH always starts QSH; no per-user shell is kept), `SYSTOOLS.LPRINTF`, `SYSTOOLS.GENERATE_SPREADSHEET` | refused SQL0084 | pub400 | |
| `STRSQL`: saved sessions (SQL6102, SQL6335, SQL7971), F13 options 1 to 4 (session attributes reshaping as pub400's, printer file, remove entries, save to a source file), SELECT output to display, printer (`QPQUPRFIL` report) and file, display formats (edited numbers, every date and time format, `*COMMA`), F19/F20 at pub400's column widths, F4 then F12 | partial: output to a file supports options 1, 2, 4, 5; OUTMBR other than `*FILE`/`*FIRST` and OUTOPT(3) refused SQL0084; the driver has not yet run at a real console | pub400 | `isql --test services`, `isql::print`, `isql::format`, `isql::result`; `golden/sql/strsql/` |
| `STRSQL` F4 prompts of `ALTER TABLE` through `SET TRANSACTION` (26 statements), the second `INSERT`, `UPDATE`, `CREATE INDEX` displays | supported as pub400 shows them; `ALTER TABLE`, `COMMENT ON`, `LABEL ON`, `RENAME`, `GRANT`/`REVOKE`, `CONNECT`, `RELEASE`/`DISCONNECT`, `CREATE PROCEDURE` build as far as their first display and leave the rest on the input lines; the column lists' other keys answer CPD6A60 | pub400 | `isql --test stmts` |
| `CONNECT`, `SET CONNECTION`, `RELEASE`, `DISCONNECT` in STRSQL | supported through the DRDA client; other statements go to the current remote server; a query there refused SQL0084 (BLOCKED(sql): no column descriptions come back) | none | |
| `DECPNT(*COMMA)` in STRSQL: a comma followed by a digit inside a number is the decimal point | supported; a second decimal point SQL0104 | pub400 | `sql::lex` |
| Command prompts of `RUNSQL`, `RUNSQLSTM`, `CRTSQLRPGI`, `STRSQL` | supported: parameters, order, defaults and labels as pub400's | pub400 | `sys-cpps --test sql_prompts` |
| Command prompts of `STRDBMON`, `ENDDBMON` (7.5 definitions: `TYPE` default `*DETAIL`), `PRTSQLINF`, `DLTSQLPKG` | supported | pub400 | `sys-cpps --test sql_prompts` |
| `RUNSQLSTM`, `RUNSQL` | partial: options checked (non-default `USRPRF`, `DYNUSRPRF`, `SAAFLAG`, `FLAGSTD` and values QS/400 cannot honour refused SQL0084; `ALWCPYDTA`, `ALWBLK`, `DLYPRP`, `SYSTIME`, `TGTRLS` without effect); a SELECT in RUNSQLSTM or RUNSQL refused SQL0084 (severity 30), as pub400; not yet run end to end | pub400 | |
| `RUNSQLSTM OUTPUT(*PRINT)` listing | partial: pub400's layout (options page, numbered source, messages, summary); message positions always 1, CCSIDs shown as 37, no "source member changed" line | pub400 | `sql::listing` |
| `CRTSQLRPGI` options | settings it does not apply refused SQL0084 (`RPGPPOPT`, `CONACC`, `DECFLTRND`, `SQLCURRULE`, `DECRESULT`, `SRTSEQ`, `DFTRDBCOL`, `DYNDFTCOL`, `SQLPATH`, `COMPILEOPT`, `DYNUSRPRF`, `SAAFLAG`, `FLAGSTD`), accepted and ignored before | none | |
| SQLRPGLE precompiler (`CRTSQLRPGI`) | partial (Phase 9) | pub400 | |
| DRDA server (JDBC, ACS) | partial: `EXCSQLSET`, `CALL` with result sets and output values (`SQLDTARD`), bind into packages; LOB locators refused SQL0084 | primary, pub400 | replay `database-jdbc`, `ddm::session` |
| `SYSIBM` metadata views `SQLTABLES`, `SQLCOLUMNS`, `SQLPRIMARYKEYS`, `SQLFOREIGNKEYS`, `SQLSCHEMAS`, `SQLTABLETYPES`, `SYSTABLETYPES`, `SQLTYPEINFO`, `SQLSTATISTICS`, `SQLSPECIALCOLUMNS`, `SQLTABLEPRIVILEGES`, `TABLES`, `COLUMNS`, `VIEWS`, `SCHEMATA` | supported: the primary's columns, rows from the QSYS2 views | primary | `sysibm/SQLMETA` |
| JDBC catalog requests (`DatabaseMetaData`) | partial: answered from the views; replies not yet byte-identical; `SQLPROCEDURES`, `SQLFUNCTIONS`, `SQLUDTS` empty | primary | replay `database-metadata` |
| Other `SYSIBM` views (`_S` forms, `ROUTINES`, `PARAMETERS`, ...) | not present: SQL0204 | primary | `sysibm/SYSIBMLS` |
| DRDA client | partial (§2) | primary | `conformance::drda_client` |
| SQL CLI (`QSQCLI`): the 40 functions of the brief's list (handles, attributes, connect, prepare, execute, describe, bind, fetch, scroll, `SQLGetData`, diagnostics, transactions), `SQLTables`, `SQLColumns`, `SQLPrimaryKeys` | supported; bound storage checked at every use | pub400 | `rpg_kit` CLI001-CLI005, `sqlcli::engine` |
| CLI server mode (`SQL_ATTR_SERVER_MODE`): statements in `QSQSRVR` prestart jobs in `QSYSWRK` | partial: matches pub400 in the kit, where a second session stands in for the job; the real prestart path not yet run on a system (NEEDS-CODY: QEMU) | pub400 | CLI001 |
| The other 81 `QSQCLI` exports (`SQLGetInfo`, `SQLMoreResults`, the wide forms, descriptors, locators) | not present: a program that imports one fails to bind (CPD5D02) | pub400 (export list) | |
| CLI divergences: `SQLGetData` of an integer as characters, the length of a cut column name, 01002 on disconnect | differ from pub400, listed in `docs/SYSAPI.md` §13 | pub400 | |
| `*SQLPKG` packages: made by the host server's package requests (JTOpen extended dynamic) and by DRDA bind; statements run by name; saved and restored | supported | primary | `sql::package`, replay `database-sqlpkg`, `ddm`, `saverst::sqlpkg` |
| `DLTSQLPKG` | supported | primary | `sys-cpps` |
| `DSPSQLPKG` | not present: no such command on V7R1M0 either (CPD0030) | primary | |
| `CRTSQLCI`, `CRTSQLCBLI` | not present | none | |
| `STRDBMON`, `ENDDBMON` (output file of the primary's 276-column `QQQDBMN` format) | partial: record types 3018, 1000, 3000, 3001, 3003, 3010, 3014; programs, host servers, DDM and QAP monitored, STRSQL, RUNSQL and RUNSQLSTM not; `TYPE(*DETAIL)` writes the same records as `*SUMMARY` (no 3019); `FTR*` filters, `FTRSQLCODE`, `JOB` operator `*NE`, `HOSTVAR` other than `*BASIC`, thresholds, `INCSYSSQL(*YES)` refused CPF9898 | primary | `sql::dbmon` |
| `CHGQRYA` | partial: `DEGREE`, `ASYNCJ`, `APYRMT` accepted without effect (hints); governor limits and `QRYOPTLIB` only at their defaults | primary | |
| `PRTSQLINF` of `*PGM`/`*SRVPGM` | supported for programs `CRTSQLRPGI` makes from now on, laid out as pub400's; of `*SQLPKG` and `*JOB` refused SQL9011 (BLOCKED(sql)) | pub400 | `rpgc::prtsqlinf` |
| `DSPFD`, `DSPDBR` of SQL tables, views, indexes; DSPDTAARA of a sequence; DSPPGM of an SQL procedure | supported (SQL file type, view statement, check option; views as dependents) | primary | `sql::sqldsp` |
| `ADDPFTRG` (`RPLTRG`, generated names, `ALWREPCHG`, `TRGUPDCND`, `THDSAFE`, `MLTTHDACN`), `RMVPFTRG`, `CHGPFTRG`, `DSPFD TYPE(*TRG)` with `OUTPUT(*OUTFILE)` (`QAFDTRG`) | supported; other `DSPFD` outfile types refused CPD0084 | primary | `db::trg`, `oracle/trigger/p8` |
| Trigger buffer | supported, byte for byte as the primary's | primary | `db::trg`, `mi::trigger` |
| `*BEFORE`/`*AFTER` x `*INSERT`/`*UPDATE`/`*DELETE` from record I/O (physical and logical files; status 01023/01024, CPF502B) | supported | primary | `mi::trigger` |
| `*READ` triggers on record I/O, logical files (one based-on member) and SQL queries (once per row the scan keeps) | supported; in a join the inner table fires for every row its scan keeps where the primary fires on matches only; refused SQL0084 where no program can be called (the service console) | primary | `sql::trigger`, `db::trg` |
| SQL changes firing external and SQL triggers | supported for statements of programs, the host server, DRDA, QAP, STRSQL, RUNSQL and RUNSQLSTM; refused SQL0084 from the service console | primary | `qrpgrt::sqltrg`, `trigger/TRGFIRE` |
| Triggers from `CPYF` (insert triggers; a failure stops the copy with CPF2972), DFU and the DDM server | supported; `CPYF MBROPT(*UPDADD)` into a file with update triggers refused CPF502B; FTP and the service console still refused | primary | `db::trg` |
| `DLTF` of a file with SQL triggers | deletes their programs | primary | `trigger/DLTFTRG` |
| `ALWREPCHG(*NO)` on after triggers | supported: the program's own change of the row is not refused, as on the primary | primary | `oracle/trigger/p9` |
| `ADDPFCST`, `RMVPFCST`, `CHGPFCST`, `DSPCPCST`, `DSPFD TYPE(*CST)` | supported, the primary's messages; not yet run end to end under QEMU | primary | `sys-cpps` |
| `WRKPFCST`, `EDTCPCST` | supported: `QSDBCST` panels as 7.1 shows them (options 2, 4, 6; a sequence verifies a constraint in check pending again); F15, F16, F17, F22 and F11 not yet | primary (`verified/oracle71/audit/wrkpfcst`, `edtcpcst`) | `uim --test audit_panels` |
| Query/400 (`WRKQRY`, `RUNQRY`, product QS00QU1) | not in this work: a follow-on product | | |

## 9a. Authority

Every SQL statement and database command checks the object authority IBM
requires (`security/sec/required.toml` holds each requirement with IBM's
in our own words); a function that checks nothing is listed in
`security/sec/exempt.toml` with the caller that checked first, and
`cargo test -p sec --test exempt` fails on any other.

| Item | Status | Oracle | Test |
|---|---|---|---|
| Authority of `NEXT VALUE`/`PREVIOUS VALUE` (USAGE: `*CHANGE`), `ALTER TABLE`, `LABEL ON TRIGGER`, `DROP` and `REPLACE` of routines (`*OBJEXIST` on the program), `FOREIGN KEY` (`*OBJREF` on the parent), SQL packages, `ADDPFTRG`/`RMVPFTRG`/`CHGPFTRG`, `ADDPFCST`/`CHGPFCST`/`RMVPFCST`/`DSPCPCST`, `CPYF` by `MBROPT` and based-on files, `RGZPFM`, `APYJRNCHG`/`RMVJRNCHG` on the files they change | supported: SQL0551 or CPF2189 | none: no profile without authority can be made on the primary; IBM's documented requirements | `sql::dbauth` |
| `*EXECUTE` on an SQL trigger's program, `*OBJEXIST` on it for `DROP TRIGGER` and `CREATE OR REPLACE TRIGGER`, `*USE` on journal receivers for `APYJRNCHG`/`RMVJRNCHG`, `*USE` on `CRTPF` for `CPYF CRTFILE(*YES)` | supported | none | `sql::dbauth` |

## 9b. Performance

| Item | Status | Oracle | Test |
|---|---|---|---|
| Join methods: nested loop, hash join, sort-merge join, chosen by cost; same rows in the same order whichever runs | supported | primary | `perf/JOINM`, `sql::planner` |
| Index-only access (fixed CHAR and signed numeric keys of a physical file's own path) | supported | none | `sql::planner` |
| Column statistics on `CREATE INDEX` and by the `QDBFSTCCOL` system job; key-range estimates from them | partial: kept in the storage service's memory, lost at IPL (BLOCKED(sql)); `SYSCOLUMNSTAT` with IBM's 45 columns lists every column, null where nothing was collected | pub400 (shape) | `sql::planner` |
| Several indexes at once (union for `OR`/`IN`, intersection for conjuncts) | supported where a full scan would run otherwise | none | `sql::planner` |
| Predicate pushdown into views, nested table expressions and CTEs; subquery decorrelation | supported for the plain forms (listed in `docs/SQL.md` history); others run as before | primary | `perf/PUSHDN`, `perf/DECORR` |
| `FETCH FIRST`/`OFFSET` stopping a lone scan early | supported | pub400 | `perf/OPTFOR` |
| Plan cache (statement text and naming, 512 MiB or 10,000 entries); `QSYS2.DUMP_PLAN_CACHE`, `CLEAR_PLAN_CACHE`, `CHANGE_PLAN_CACHE_SIZE` (`*JOBCTL`) | partial: the parsed statement is cached, the access plan chosen again each run; the entry-selection parameters refused SQL0084; `QIBM_DB_SQLADM` not accepted (no function usage registry) | pub400 (names) | `sql::planner` |
| QAQQINI `QUERY_TIME_LIMIT`, `QUERY_STORAGE_LIMIT`, `STORAGE_LIMIT` (SQL0666) | supported: predictive governor | pub400 | `perf/GOVERN` |
| Spilling temporary results | partial: `ORDER BY` spills; a hash join without room becomes a sort-merge join; join, grouping and `DISTINCT` rows stay in memory under the SQL0904 limit | none | |

## 10. Refused

Every form the engine refuses, with its SQLCODE, is in the tables above
until the work ends; this section then lists the remaining refusals
verbatim for the release notes.

## 11. Benchmarks

Recorded by `cargo run -p build -- bench` (release, host-side, a store in
memory; Windows 11, 2026-11-05):

| Benchmark | Time |
|---|---|
| Load 1,000,000 rows, 1,000 a statement | 10.7 s |
| `SELECT * ... ORDER BY V` over 1,000,000 rows | 11.4 s |
| Three-way join of 1,000,000 x 1,000 x 10 rows, grouped | 14.6 s |
| Recursive CTE of 99,999 levels | 1.2 s |
| `SUM() OVER (PARTITION BY ... ORDER BY ... ROWS UNBOUNDED PRECEDING)` over 100,000 rows | 3.2 s |
| An SQL procedure calling another 10,000 times | 0.96 s |
| A 100 MB CLOB inserted / read back | 4.3 s / 0.55 s |

Phase 3: the database kit's outputs are identical before and after; the
`db_bench` (20,000 rows) path does not touch the planner. A member whose
records and key grow together reaches the store's extent limit near
853,000 rows (the bench creates its key after the load); see the open
items.

## Acceptance

Filled in when the work ends.
