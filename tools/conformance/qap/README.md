# QAP live checklist setup

QS/400 Access's live checklist runs against a real QS/400 with the same
database its in-process tests get from `qapd::testing`
(`net/qapd/src/testing/sql/fixtures.rs`). These files make it.

| File | What |
| --- | --- |
| `hlinv.sql` | `RUNSQLSTM` script: schemas `HLINV`, `HLTEST`, `HLPAY`, `HLMANY` and `ARCH001` to `ARCH140`; `HLINV.ASSETS` (the twelve assets), `LOCATIONS`, the view `ASSETSACT`; `HLTEST.ALLTYPES`, `JRN`, `NOJRN`, `SLOW`, `SLOWLOG`, `BIG` (over 1 MiB); `HLPAY.SALARIES`; `HLMANY.T001` to `T150` |
| `hlinv.cl` | The CL that SQL cannot say: `NOJRN` unjournaled, profiles `BOB` and `ANN`, `SALARIES` for `ANN` only |
| `hlinv-types.sql` | **Not runnable yet.** `HLTEST.NEWTYPES` (`DECFLOAT`, `CLOB`, `BINARY`, `VARBINARY`, `BLOB`, `TIMESTAMP(12)`, `BOOLEAN`) and the alias `HLINV.HW`, for when the engine has them |

## Running it

As `QSECOFR` (or a profile with `*ALLOBJ` and `*SECADM`), on a system
where none of the schemas exist:

1. Put the files in the IFS, for example with `sftp` or the client's IFS
   task, in `/home/QSECOFR/`.
2. `RUNSQLSTM SRCSTMF('/home/QSECOFR/hlinv.sql') COMMIT(*NONE) NAMING(*SQL)`.
   It ends at the first error (`ERRLVL(10)`); the listing says which.
3. Each command of `hlinv.cl`, one at a time (Command Entry, or the
   client's Run Command). Change the two passwords first.
4. Once the engine has the new types: `RUNSQLSTM
   SRCSTMF('/home/QSECOFR/hlinv-types.sql') COMMIT(*NONE) NAMING(*SQL)`,
   and remove the `#[ignore]` from `hlinv_types_runs_after_hlinv` in
   `tools/conformance/tests/qap_setup.rs`.

To run it again, delete the libraries first (`DLTLIB HLINV`, and so on).

## What differs from `qapd::testing`

`CREATE SCHEMA` journals every table made in it to the schema's
`QSQJRN`, so `JRN` needs nothing and `NOJRN` is ended by `hlinv.cl`.
Where today's engine cannot hold the fixture's values, the script keeps
the column and uses another value, each noted in the script: no E
notation in `REAL` and `DOUBLE` literals, `SMALLINT` and `INTEGER` to 4
and 9 digits, graphic literals within CP037. A column's long name takes
a system name (`MANUFACTURER FOR COLUMN MANUFACT`), the view is
`ASSETSACT` (the fixture's `ASSETS_ACTIVE`), and the fixture's logical
file `ASSETSL1` has no SQL statement. `SLOW` is not slow by itself: join
three copies (`SELECT COUNT(*) FROM HLTEST.SLOW A, HLTEST.SLOW B,
HLTEST.SLOW C`) for a query to cancel.

`tools/conformance/tests/qap_setup.rs` runs the script over a store in
memory, as `RUNSQLSTM` does: `cargo test -p conformance --test qap_setup`.
