# The SQL conformance kit

Case files of SQL statements, run here in memory and on an oracle over
JDBC, their outcomes compared line by line. `docs/SQL.md` names the case
behind each row of the feature matrix.

## Cases

`cases/<area>/<NAME>.sql`, `NAME` upper case and at most 10 characters.

- The first lines are directives: `-- oracle: primary` (Cody's V7R1M0
  box; anything Db2 for i 7.1 has), `-- oracle: pub400` (V7R5M0; 7.2 and
  later), or `-- oracle: none <reason>` (QS/400 behaviour with no oracle;
  its outcome is kept by hand).
- A statement ends with `;` at the end of a line. `--#SET TERMINATOR @`
  changes the terminator (SQL PL bodies) until the next
  `--#SET TERMINATOR ;`.
- `{lib}` is the library a case may write in: `QSTEST` on the primary,
  the user's library on pub400, `QSTEST` in a fresh store here. A case
  cleans up after itself.

Statements run in order on one connection: `*SQL` naming, autocommit,
current schema `{lib}`, ISO dates and times.

## Outcomes

`expected/<area>/<NAME>.out` is the oracle's outcome, `target/conformance-sql/ours/`
ours. Per statement:

```
> first line of the statement (72 characters)
sqlcode N sqlstate S msgid SQLnnnn|- [count N]
warning CODE STATE MSGID            (one per JDBC warning, positive SQLCODEs)
# COLUMN TYPE[(p[,s])] [NOT NULL]   (one per result column)
| value | value |                   (one per row)
```

`count` is shown for `INSERT`, `UPDATE`, `DELETE` and `MERGE`. Values:
`NULL`; numbers as JTOpen's `getString` gives them (decimals to their
scale); `DOUBLE` and `REAL` as Java's `Double.toString`/`Float.toString`;
character, date and time values quoted, blanks kept; timestamps as
`YYYY-MM-DD-HH.MM.SS.ffffff`; binary as `X'..'`. The library and the
account name are written `{lib}` and `{user}`. The first line, `--
captured from ...`, is not compared.

## Running

```
cargo test -p conformance --test sql_kit                 # every case here, compared
QS400_SQL_ONLY=ddl/ cargo test -p conformance --test sql_kit
tools/conformance/sql/capture.sh [CASE.sql...]           # capture uncaptured cases
```

`capture.sh` needs a JDK and `jt400.jar` (`target/jtopen/jt400.jar`, from
Maven Central `net.sf.jt400:jt400`, or `JT400_JAR`), and the oracle
credentials in `tools/conformance/oracle/.env`. `QS_SQL_DIRECT=1` connects
to the database server port directly when the sign-on server will not
answer. pub400 is throttled: capture there in small batches.
