      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     ICUSTR
     I              NAME                        CNAME
     C                   READ      CUSTR                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(CUSNO) + ' ' + %TRIM(CNAME)
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTR                                  99
     C                   ENDDO
     C                   EVAL      *INLR = *ON
