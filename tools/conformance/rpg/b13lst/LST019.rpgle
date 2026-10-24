      * Compile-time arrays and tables.
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DARR              S              4    DIM(3) CTDATA PERRCD(1)
     DTABX             S              3    DIM(2) CTDATA PERRCD(1)
     DTABY             S              5    DIM(2) ALT(TABX)
     DNUMS             S              3S 0 DIM(4) CTDATA PERRCD(2)
     C                   EVAL      LINE = ARR(1) + ARR(3) + TABX + TABY
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
**CTDATA ARR
aaaa
bbbb
cccc
**CTDATA TABX
AAAone
BBBtwo
**CTDATA NUMS
001002
003004
