# QSTEST conformance kit

The fixed library every database test uses, on QS/400 and on the real
IBM i. `dds/` holds one DDS member per file (`NAME.PF`, `NAME.LF`),
`data/` one CSV per physical file, `rla/qstest.rla` the record-level
script, `sql/queries.sql` the fixed queries, `oracle/` the CL and RPG that
produce the machine's outputs, and `expected/` is where those outputs go.

`cargo test -p conformance` builds the library in memory from these
files, runs the script and the queries, writes everything it produced
under `target/conformance-db/ours/`, and compares with `expected/` file
by file. A missing expected file is reported and skipped, never failed.
The QEMU harness drives the same kit through the service console and
compares with the in-memory run.

The files, their keys and what they cover:

| File | Kind | Covers |
|---|---|---|
| CUSTOMER | PF, UNIQUE CUSNO | packed, char, DFT, null date, zoned, VARLEN |
| ORDERS | PF, UNIQUE ORDNO | binary key, date, time, null timestamp |
| ORDLINE | PF, UNIQUE ORDNO LINE | two-field key |
| PRODUCT | PF, UNIQUE PRODID | single and double float |
| INVENTORY | PF, arrival | no key, null date |
| EVENTS | PF, keyed | every DATFMT and TIMFMT |
| TYPES | PF, keyed | every field type, null capable |
| NOTES | PF, UNIQUE | long VARLEN, null text |
| LOGHIST | PF, REUSEDLT(*YES) | reused deleted records |
| RLALOG | PF, arrival | the oracle's script log |
| QDDSSRC | source PF | the DDS members |
| CUSBYNAME, ORDBYCUS, PRDBYDESC | LF, keyed | key order, DESCEND |
| CUSORD, ORDALL | LF, multi-format | format tags in one path |
| ORDJOIN | LF, join | ORDERS to CUSTOMER |
| ACTIVE | LF, select/omit | static selection |
| BIGORD | LF, DYNSLT | dynamic selection |
