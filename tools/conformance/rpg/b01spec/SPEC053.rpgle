      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   F   40        DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     ICUSTF     AA  01   24 CB
     I         AND       25 Co
     I                                  4   23  NAME
     ICUSTF     AB  02   24 CD
     I         OR        24 CA
     I                                  4   23  NAME
     ICUSTF     AC  03
     I                                  4   23  NAME
     C                   READ      CUSTF                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %TRIM(NAME) + ' ' + *IN01 + *IN02 +
     C                             *IN03
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTF                                  99
     C                   ENDDO
     C                   EVAL      *INLR = *ON
