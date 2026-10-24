     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DARR              S              3  0 DIM(5) CTDATA PERRCD(5)
     DK                S              3  0 INZ(9)
     DI                S              3  0 INZ(1)
     C                   XFOOT     ARR           TOT               5 0
     C                   SORTA     ARR
     C     K             LOOKUP    ARR(I)                                 50
     C                   EVAL      LINE = %CHAR(TOT) + ' ' + %CHAR(ARR(1)) + ' '
     C                             + %CHAR(ARR(5)) + ' ' + *IN50 + ' ' +
     C                             %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA ARR
005003009001007
