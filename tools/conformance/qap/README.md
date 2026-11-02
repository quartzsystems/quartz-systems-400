# QAP live checklist setup

QS/400 Access's live checklist runs against a real QS/400 with the same
database its in-process tests get from `qapd::testing`
(`net/qapd/src/testing/sql/fixtures.rs`). These files make it.

| File | What |
| --- | --- |
| `hlinv.sql` | `RUNSQLSTM` script: schemas `HLINV`, `HLTEST`, `HLPAY`, `HLMANY` and `ARCH001` to `ARCH140`; `HLINV.ASSETS` (the twelve assets), `LOCATIONS`, the view `ASSETSACT`; `HLTEST.ALLTYPES` (one column of every type: `DECFLOAT` with its special values, a `CCSID 65535` `CHAR`, `BINARY`, `VARBINARY`, `BLOB`, `CLOB`, `TIMESTAMP(12)`, `BOOLEAN`, a row of nulls), `JRN`, `NOJRN`, `SLOW`, `SLOWLOG`, `BIG` (over 1 MiB); `HLPAY.SALARIES`; `HLMANY.T001` to `T150` |
| `hlinv.cl` | The CL that SQL cannot say: `NOJRN` unjournaled, profiles `BOB` and `ANN`, `SALARIES` for `ANN` only |

## Running it

As `QSECOFR` (or a profile with `*ALLOBJ` and `*SECADM`), on a system
where none of the schemas exist:

1. Put the files in the IFS, for example with `sftp` or the client's IFS
   task, in `/home/QSECOFR/`.
2. `RUNSQLSTM SRCSTMF('/home/QSECOFR/hlinv.sql') COMMIT(*NONE) NAMING(*SQL)`.
   It ends at the first error (`ERRLVL(10)`); the listing says which.
3. Each command of `hlinv.cl`, one at a time (Command Entry, or the
   client's Run Command). Change the two passwords first.
To run it again, delete the libraries first (`DLTLIB HLINV`, and so on).

## What differs from `qapd::testing`

`CREATE SCHEMA` journals every table made in it to the schema's
`QSQJRN`, so `JRN` needs nothing and `NOJRN` is ended by `hlinv.cl`.
Where today's engine cannot hold the fixture's values, the script keeps
the column and uses another value, each noted in the script: literals of
at most 63 digits (no `1E300`), graphic literals within CP037. The engine parses `CREATE ALIAS`
but does not run it yet, so the fixture's alias `HLINV.HW` is left out. A column's long name takes
a system name (`MANUFACTURER FOR COLUMN MANUFACT`), the view is
`ASSETSACT` (the fixture's `ASSETS_ACTIVE`), and the fixture's logical
file `ASSETSL1` has no SQL statement. `SLOW` is not slow by itself: the
query in the script's comment above it nests two correlated `EXISTS`
over it and runs long a row at a time. Do not cancel-test with a join of
several copies: the engine builds a join's rows in the storage
service's memory, and an allocation that fails ends the service (and
the IPL).

`tools/conformance/tests/qap_setup.rs` runs the script over a store in
memory, as `RUNSQLSTM` does: `cargo test -p conformance --test qap_setup`.
