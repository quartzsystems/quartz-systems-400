     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DAMT              S              5  2 DIM(4) CTDATA PERRCD(2)
     C                   EVAL      LINE = %CHAR(AMT(1)) + ' ' + %CHAR(AMT(2)) +
     C                             ' ' + %CHAR(AMT(3)) + ' ' + %CHAR(AMT(4))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA AMT
0012500050
1000099999
