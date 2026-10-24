      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   E           K DISK
     F                                     PREFIX(C_)
     F                                     RENAME(CUSTR:CREC)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   READ      CREC                                   99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(C_CUSNO) + ' ' + %TRIM(C_NAME) +
     C                             ' ' + %CHAR(C_BAL)
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CREC                                   99
     C                   ENDDO
     C                   EVAL      *INLR = *ON
