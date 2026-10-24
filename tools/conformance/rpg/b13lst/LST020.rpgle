      * The old ** separators, in definition order.
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA1               S              2    DIM(2) CTDATA PERRCD(2)
     DA2               S              3    DIM(1) CTDATA
     C                   EVAL      LINE = A1(1) + A1(2) + A2(1)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
**
xxyy
**
zzz
