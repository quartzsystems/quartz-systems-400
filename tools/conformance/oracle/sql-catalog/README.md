# SQL catalog, label and default oracle captures

Cases in the SQL kit's form (`tools/conformance/sql/README.md`) run on Cody's
V7R1M0 box with `tools/conformance/sql/oracle/SqlKit.java` (library `QSTEST`,
written `{lib}`; the account `{user}`), and what came back. They settle what the
kit's cases left open; the engine follows them.

| Capture | What it settles |
|---|---|
| `CATLAY` | the columns of `QSYS2.SYSCOLUMNS`, `SYSTABLES`, `SYSINDEXES`, `SYSVIEWS` and `SYSSEQUENCES` in order, with their types, lengths, nullability and CCSIDs (`VARG` and `DBCLOB` in 1200 or 13488) |
| `PRB1` | `START WITH` below `MINVALUE` accepted (an ascending sequence gives it, then on; a descending one has no values, `SQL0359`); above `MAXVALUE` `SQL0846`; generated system names of long and delimited sequence names; defaults that do not fit (`SQL0574`, dates `SQL0180`/`SQL0181`, `USER` into fewer than 18); `GENERATED ALWAYS AS (USER)` is `SQL0104` on 7.1 |
| `PRB2` | a heading over 60 or a column text over 50 characters `SQL0107`; `BLOB()` (types, a number `SQL0171`); a delimited name's heading keeps its quotes; generated column names of a view's delimited result names (its first statement fails on 7.1, so the labels after it find no table) |
| `PRB3` | `SYSCOLUMNS` of graphic, binary, LOB and CCSID-tagged columns (storage, character and octet lengths); identity, `ROWID` and row change timestamp columns (`HAS_DEFAULT` `I`, `A`, `E`); LOB keys `SQL0350`; `LABEL`/`COMMENT ON SEQUENCE` and `SQL0107` for its text |
| `PRB4` | `SYSCOLUMNS` of every numeric, character and date type (lengths, precisions, radix, storage, `DATETIME_PRECISION`); `COLUMN_DEFAULT` spellings (`CURRENT_DATE`); `HAS_DEFAULT` `Y` for a nullable column without a default; headings default to the column name, `LABEL ... IS ''` leaves none; `COMMENT ON` is `LONG_COMMENT` apart from the label, `''` drops it; `LABEL ON INDEX` of a table `SQL0159`; a missing column `SQL0205` in labels, keys and `ALTER`, `SQL0206` in `CREATE INDEX` and queries |
| `PRB5` | `SELECT *` of `SYSTABLES`, `SYSVIEWS`, `SYSINDEXES` and `SYSCOLUMNS` for a table, views (plain, grouped, joined, with an expression), indexes and an alias; a DDS file's columns (no heading, defaults `0` and blanks) |
| `PRIVS` | (run with `SqlProbe`, 2026-11-06) a table made in `QSTEST` has `*PUBLIC *EXCLUDE` on the primary (the library's create authority); `SQLTABLEPRIVILEGES` lists the owner's `ALTER`, `DELETE`, `INSERT`, `REFERENCES`, `SELECT`, `UPDATE` (no `INDEX`) and no `*PUBLIC` row; `SQLCOLPRIVILEGES` the owner's `INSERT`, `REFERENCES`, `SELECT`, `UPDATE` per column |
| `PRIVG` | (`SqlProbe`, 2026-11-06) after `GRANT SELECT ... TO PUBLIC` a `PUBLIC SELECT NO` row; `SQLSPECIALCOLUMNS` with identifier type 2 (row version) has no rows for a table without a row change timestamp, type 1 the primary key; `QSYS2.SYSTABAUTH` and `SYSCOLAUTH` are not on V7R1M0 |
