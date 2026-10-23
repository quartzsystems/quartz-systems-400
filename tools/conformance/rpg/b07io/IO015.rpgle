      *; dds: CUSTF
      *; sqlfile: CUSTF
      * A program-described input file: I specs over CUSTF.
     FCUSTF     IF   F   40        DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     ICUSTF     NS
     I                             P    1    3 0CNO
     I                                  4   23  CNAME
     I                                 24   35  CCITY
     I                             P   36   40 2CBAL
     I                                  4    4  CFIRST
     C                   READ      CUSTF                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(CNO) + ' ' + CNAME + CCITY +
     C                             %CHAR(CBAL) + ' ' + CFIRST
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTF                                  99
     C                   ENDDO
     C                   SETON                                        LR
