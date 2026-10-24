      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   F   40        DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     ICUSTF     NS  01
     I                             P    1    3 0CUSNO
     I                                  4   23  NAME
     I                                 24   35  CITY
     I                             P   36   40 2BAL
     C                   READ      CUSTF                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(CUSNO) + ' ' + %TRIM(NAME) + '/'
     C                             + %TRIM(CITY) + ' ' + %CHAR(BAL) + ' ' +
     C                             *IN01
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTF                                  99
     C                   ENDDO
     C                   EVAL      *INLR = *ON
