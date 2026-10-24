      * Names of more than fifteen characters.
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA_REALLY_LONG_NAME_FOR_A_FIELD...
     D                 S              5P 0 INZ(1)
     DANOTHER_VERY_LONG_NAME_HERE...
     D                 S             10    INZ('x')
     C                   EVAL      LINE = %CHAR(A_REALLY_LONG_NAME_FOR_A_FIELD)
     C                             + ANOTHER_VERY_LONG_NAME_HERE
     C                   WRITE     QSYSPRT       LINE
       A_REALLY_LONG_NAME_FOR_A_FIELD += 1;
     C                   SETON                                        LR
