# The kit on pub400.com

`gen.py` builds a variant of the QSTEST kit for an account on pub400.com
(IBM i 7.5, job CCSID 273) that cannot create libraries, and the
transfer script that loads, runs and fetches it. It was run on
2026-10-22; the captures it adds to `../expected/` are the ones the 7.1
box could not make: `rla.csv`, `dspjrn.csv` and the exports of
`PRODUCT`, `ORDBYCUS`, `ORDJOIN` and `PRDBYDESC`.

What differs from `../QSTEST.CLLE`:

- the kit lives in library `CWELLMANB` (cleared, then first on the
  library list) and under `/home/CWELLMAN/qstest`; the driver program
  goes to `CWELLMAN1` so clearing the kit never deletes it;
- the DDS names its files unqualified, since the longer library name
  pushes `JFILE` past column 80;
- `CRTBNDRPG ... TGTCCSID(*JOB)`: the compiler will not take a UTF-8
  stream with the default target;
- the `TYPES` import may fail: 7.5 will not convert into the hex field
  `FH` (CCSID 65535), and `TYPES` and query 9 come from the 7.1 capture;
- `CPYTOIMPF ... ADDCOLNAM(*SYS)` writes the column names that 7.1
  needed `QHEADERS.SQL` for.

To run it (JTOpen from Maven Central, `net.sf.jt400:jt400`, in `lib/`):

```
python -I gen.py
javac -cp lib/jt400.jar -d . Xfer.java Help.java
java -cp "lib/jt400.jar;." Xfer pub400.com USER PASSWORD script run.xfer
```

The outputs land in `got/`. `Help` runs commands and prints each
message's second-level text, which is how the CCSID and compile errors
above were read.
