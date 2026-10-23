# Producing the oracle outputs on IBM i

1. Send the kit to the machine: `dds/*` to `/home/QSTEST/dds/`, `data/*`
   to `/home/QSTEST/data/`, `oracle/RLATEST.RPGLE` and `oracle/QRESULTS.SQL`
   to `/home/QSTEST/oracle/` (the CL reads both from there), and
   `oracle/QSTEST.CLLE` to a source file of your choice with record
   length 240 (its lines run past 112).
2. `CRTBNDCL PGM(QGPL/QSTEST)` from `QSTEST.CLLE`, outside `QSTEST`
   because the program deletes and recreates that library, then
   `CALL QGPL/QSTEST`. The program creates the library, loads the data,
   runs the script and writes every output to `/home/QSTEST/out/`.
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

## Captured 2026-10-22 on IBM i 7.1 (V7R1M0)

The box had no RPG compiler, so `RLATEST` did not run: `rla.csv`,
`dspjrn.csv` (no record entries) and the files the script rewrites
(`PRODUCT`, `ORDBYCUS`, `ORDJOIN`, `PRDBYDESC`) are not in `expected/`.
7.1 has no `ADDCOLNAM`, so the outfiles' column names come from
`QHEADERS.SQL` (`*.hdr`, position and name) and were put on the first
line by hand; `DSPFFD FILE(*ALL)` lists files alphabetically and
includes the `QRESnn` tables and the outfiles themselves, so `ffd.csv`
and `lvlid.csv` were cut to the kit's files and put in the kit's order.
What differs from this implementation: a `TIMFMT(*USA)` time field
keeps its seconds on the machine (`EVENTS`, `Q08`), `CPYTOIMPF` writes
a hex (`H`) field's bytes unconverted (`TYPES`), `DSPFFD` reports 0
digits for a character field (`WHFLDD`), and the format level
identifiers (`RFID`) are IBM's own hash.

## Captured 2026-10-22 on pub400.com (IBM i 7.5)

The rest of the kit ran on pub400 with the RPG step (`pub400/README.md`
says how): `rla.csv`, `dspjrn.csv` and the exports of `PRODUCT`,
`ORDBYCUS`, `ORDJOIN` and `PRDBYDESC` in `expected/` come from there.
What the machine settled: `%FOUND`, `%EQUAL` and `%EOF` keep their
values between operations; `SETLL` and `SETGT` with no record report
`00012`; a `READ` after a `READE` mismatch is end of file; `CPYTOIMPF`
writes arrival sequence even for keyed files, and a join file in the
order of a keyed path over its primary that leads with the join field.
`RLALOG` is not exported: it is `rla.csv`.
