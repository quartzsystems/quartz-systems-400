      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DKEY              S              5  0
     C     CKEY          KLIST
     C                   KFLD                    KEY
     C                   EVAL      KEY = 105
     C     CKEY          CHAIN     CUSTR                              90
     C                   IF        *IN90 = *OFF
     C                   EVAL      LINE = %TRIM(NAME) + ' ' + %TRIM(CITY)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   EVAL      KEY = 999
     C     CKEY          CHAIN     CUSTR                              90
     C                   EVAL      LINE = 'nr=' + *IN90
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
