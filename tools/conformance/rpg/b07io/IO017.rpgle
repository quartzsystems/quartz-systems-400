      *; dds: ORDF
      *; sqlfile: ORDF
      * Fixed form: KLIST and KFLD, CHAIN and READE with indicators.
     FORDF      IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DKCUS             S              5P 0
     DKORD             S              5P 0
     C     ORDKEY        KLIST
     C                   KFLD                    KCUS
     C                   KFLD                    KORD
     C     PARTKY        KLIST
     C                   KFLD                    KCUS
     C                   EVAL      KCUS = 101
     C                   EVAL      KORD = 2
     C     ORDKEY        CHAIN     ORDF                               50
     C                   EVAL      LINE = 'chain ' + *IN50 + ' ' + ITEM
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      KORD = 9
     C     ORDKEY        CHAIN     ORDR                               50
     C                   EVAL      LINE = 'chain ' + *IN50 + ' ' + ITEM
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      KCUS = 104
     C     PARTKY        SETLL     ORDF                                   51
     C     PARTKY        READE     ORDF                                   52
     C     *IN52         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(ORDNO) + ' ' + ITEM + ' ' +
     C                             *IN51
     C                   WRITE     QSYSPRT       LINE
     C     PARTKY        READE     ORDF                                   52
     C                   ENDDO
     C                   SETON                                        LR
