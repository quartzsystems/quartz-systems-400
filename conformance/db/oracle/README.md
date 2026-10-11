# Producing the oracle outputs on IBM i

1. Send the kit to the machine: `dds/*` to `/home/QSTEST/dds/`, `data/*`
   to `/home/QSTEST/data/`, `oracle/QSTEST.CLLE` and `oracle/RLATEST.RPGLE`
   to a source file of your choice (record length 112).
2. `CRTBNDCL QSTEST/QSTEST` from `QSTEST.CLLE`, then `CALL QSTEST/QSTEST`.
   The program creates the library, loads the data, runs the script and
   writes every output to `/home/QSTEST/out/`.
3. Copy `/home/QSTEST/out/*` into `expected/` here, keeping the names:

| expected file | from |
|---|---|
| `data/NAME.csv` | `CPYTOIMPF` of each physical and single-format logical file |
| `rla.log` | `CPYTOIMPF` of `RLALOG` |
| `ffd.csv` | `DSPFFD OUTFILE` copied with `CPYTOIMPF` (QADSPFFD columns) |
| `lvlid.csv` | `DSPFD TYPE(*RCDFMT) OUTFILE` copied with `CPYTOIMPF` |
| `dspjrn.csv` | `DSPJRN OUTFILE` copied with `CPYTOIMPF` (QADSPJRN columns) |
| `sql/Qnn.csv` | `CPYTOIMPF` of each `QRESnn` the CL builds from `sql/queries.sql` |

The comparer reads the IBM outfile columns it needs (`WHNAME`, `WHFLDT`,
`WHFLDB`, `WHFLDD`, `WHFLDP`, `WHFOBO`, `WHNULL`; `RFNAME`, `RFID`;
`JOCODE`, `JOENTT`, `JOOBJ`, `JOMBR`, `JOCTRR`) and masks timestamps and
job names. Anything the CL cannot produce is simply left out; the test
names what is missing.

`RLATEST.RPGLE` is generated from `rla/qstest.rla`; regenerate it with
`QS400_UPDATE_ORACLE=1 cargo test -p conformance` after changing the
script. It is a starting point: the machine's compiler may want tweaks,
and the status codes it logs are the truth the script's `=>` column is
then corrected to.
