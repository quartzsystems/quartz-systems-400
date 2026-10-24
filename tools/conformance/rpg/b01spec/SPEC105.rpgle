     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA1               S              2    DIM(3) CTDATA PERRCD(3)
     DA2               S              3  0 DIM(2) CTDATA PERRCD(1)
     C                   EVAL      LINE = A1(1) + A1(2) + A1(3) + ' ' +
     C                             %CHAR(A2(1) + A2(2))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**   the first array
aabbcc
**
001
002
