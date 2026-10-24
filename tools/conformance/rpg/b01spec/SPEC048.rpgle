     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DWHOLE            DS
     D ALL                           10    INZ('abcdefghij')
     D MID                            3    OVERLAY(ALL:
     D                                     5)
     D ARR                            2    DIM(5)
     D                                     OVERLAY(ALL)
     C                   EVAL      LINE = MID + ' ' + ARR(2) + ' ' + ARR(5) +
     C                             ' ' + %CHAR(%ELEM(ARR))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
