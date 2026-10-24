      *; dds: CUSTF
      *; sqlfile: CUSTF
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCUST           E DS                  EXTNAME(CUSTF)
     DKEYS           E DS                  EXTNAME(CUSTF:*KEY) QUALIFIED PREFIX(K_)
     C                   EVAL      LINE = %CHAR(CUSNO) + NAME +
     C                             %CHAR(KEYS.K_CUSNO)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
