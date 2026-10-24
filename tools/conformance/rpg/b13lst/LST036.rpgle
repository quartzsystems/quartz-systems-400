      *; dds: CUSTF
      *; sqlfile: CUSTF
      *; run: no
      * KLIST, PLIST, *ENTRY.
     FCUSTF     IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DKEY              S              5P 0
     C     *ENTRY        PLIST
     C                   PARM                    PARMIN            5 0
     C     CK            KLIST
     C                   KFLD                    KEY
     C                   Z-ADD     PARMIN        KEY
     C     CK            CHAIN     CUSTR                              90
     C                   CALL      'LSTNONE'     PL                     91
     C     PL            PLIST
     C                   PARM                    KEY
     C                   EVAL      LINE = NAME + *IN90 + *IN91
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
