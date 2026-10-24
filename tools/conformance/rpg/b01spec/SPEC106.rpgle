     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTABCOD           S              1    DIM(3) CTDATA PERRCD(1)
     DTABNAM           S              6    DIM(3) ALT(TABCOD)
     C     'B'           LOOKUP    TABCOD        TABNAM                   50
     C                   EVAL      LINE = TABCOD + ' ' + TABNAM + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     'Z'           LOOKUP    TABCOD        TABNAM                   50
     C                   EVAL      LINE = TABCOD + ' ' + TABNAM + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA TABCOD
ARed
BBlue
CCyan
