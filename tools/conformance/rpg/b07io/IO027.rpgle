      *; dds: CUSTF
      *; sqlfile: CUSTF
      * Fixed form: I specs renaming fields of an external file.
     FCUSTF     IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     ICUSTR         01
     I              CUSNO                       NUMBER
     I              BAL                         AMOUNT              111213
     C                   READ      CUSTF                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(NUMBER) + ' ' + NAME +
     C                             %CHAR(AMOUNT) + ' ' + *IN01 + *IN11 + *IN12 +
     C                             *IN13
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTF                                  99
     C                   ENDDO
     C                   SETON                                        LR
