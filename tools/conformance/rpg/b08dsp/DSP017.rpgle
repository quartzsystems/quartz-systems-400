      *; dds: B8D
      *; keys: 3,10=FIX f5
      *; keys: f12
      *; keys: f3
     H DFTACTGRP(*NO)
     FB8D       CF   E             WORKSTN
      * Fixed form: EXFMT with the key indicators.
     C                   EVAL      RES = 'Fixed form'
     C     *IN03         DOUEQ     *ON
     C                   EXFMT     KEYS
     C   KE              EVAL      RES = 'KE ' + VAL
     C   KL              EVAL      RES = 'KL ' + VAL
     C                   ENDDO
     C                   SETON                                        LR
