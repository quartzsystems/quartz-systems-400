     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DLIM              S              3  0 DIM(4) CTDATA PERRCD(4) DESCEND
     DK                S              3  0 INZ(250)
     C                   EVAL      LINE = %CHAR(%LOOKUPLE(K:LIM)) + ' ' +
     C                             %CHAR(%LOOKUPGE(K:LIM)) + ' ' +
     C                             %CHAR(%LOOKUP(K:LIM))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA LIM
900500100010
