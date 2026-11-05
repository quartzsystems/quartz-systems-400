# SQL DDL oracle captures

Scripts run with `tools/conformance/jtopen/src/DdlProbe.java` (JTOpen JDBC, `*SQL`
naming, autocommit) and what came back. `primary/` is Cody's V7R1M0 box (library
`QSTEST`), `pub400/` is pub400.com V7R5M0 (the user's library, written `{user}1`).
Message text is cut at the cause.

| Capture | What it settles |
|---|---|
| `primary/names1` | generated system names of tables, views, indexes, aliases and columns; delimited names; RENAME keeps the system name; FOR SYSTEM NAME not in 7.1 |
| `primary/v1` | SYSVIEWS and SYSVIEWDEP columns and rows, VIEW_DEFINITION text, CHECK_OPTION N/C/Y, SQL0161, DROP ... RESTRICT (SQL0616), CASCADE, OR REPLACE (views and aliases only in 7.1), sparse and expression indexes in SYSINDEXES/SYSKEYS, EVI and INCLUDE |
| `primary/g5` | GRANT/REVOKE to object authorities (DSPOBJAUT outfile), SQL0569 |
| `pub400/pub1` | FOR SYSTEM NAME, FOR COLUMN, SQL7029, OR REPLACE TABLE (ON REPLACE), CREATE TABLE AS/LIKE, DEFINITION ONLY, SQL0153, DECLARE GLOBAL TEMPORARY TABLE (SQL0601, WITH REPLACE, SQL0079), SYSTABAUTH and SYSCOLAUTH columns, RENAME ... TO SYSTEM NAME, DROP SCHEMA of a missing schema |

## Facts

- A long SQL name gets the system name of its first five characters and a five-digit
  number, the lowest not yet taken in the library (`CUSTOMER_MASTER_FILE` is
  `CUSTO00001`, the next `CUSTO00002`; `A_LONGER_NAME_X` is `A_LON00001`). Tables, views,
  indexes and aliases share the numbers. A delimited name that fits in ten characters
  with its quotes keeps them (`"lowtab"`); a longer one is four characters and four
  digits inside quotes, blanks as `_` (`"a_lo0001"`).
- Columns: the same five-plus-five rule per table, upper cased (`"lower"` is
  `LOWER00001`, `"Mixed Case Column"` is `MIXED00001`). An index key expression is
  `IXCOL00001`.
- RENAME to a long name keeps a generated system name that already has the new name's
  prefix (`CUSTO00002`), else generates one (`QSP1C` to `QSP1C00001`); to a short name
  the system name becomes it. `TO SYSTEM NAME x` changes only the system name.
- FOR SYSTEM NAME on an SQL name that is itself a valid system name: SQL7029, 428B8.
- CREATE TABLE in a library that is not a schema: +7905, 01567 (not journaled), also
  for DECLARE GLOBAL TEMPORARY TABLE; none for CREATE TABLE QTEMP.x or NOT LOGGED.
- GRANT: SELECT is `*OBJOPR *READ`, INSERT `*OBJOPR *ADD`, UPDATE `*OBJOPR *UPD`,
  DELETE `*OBJOPR *DLT`, ALTER and INDEX `*OBJALTER`, REFERENCES `*OBJREF`, ALL all of
  those, WITH GRANT OPTION adds `*OBJMGT`. Column privileges show the data authority as
  `/` (partial). REVOKE takes the data authority away and `*OBJOPR` with the last one;
  REVOKE of a privilege not held: +569, 01006. REVOKE ALL FROM PUBLIC leaves `*EXCLUDE`.
- SYSTABAUTH lists one row per privilege (SELECT, INSERT, UPDATE, DELETE, REFERENCES,
  ALTER, INDEX), IS_GRANTABLE `YES` with `*OBJMGT`, GRANTOR null.
