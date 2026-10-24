      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     ICUSTR
     I              BAL                         BAL                 212223
     C                   READ      CUSTR                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(CUSNO) + ' ' + *IN21 + *IN22 +
     C                             *IN23
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTR                                  99
     C                   ENDDO
     C                   EVAL      *INLR = *ON
