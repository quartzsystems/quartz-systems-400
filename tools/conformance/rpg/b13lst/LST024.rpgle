      * Indicators and labels in the cross-reference.
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA                S              5P 0 INZ(3)
     DB                S              5P 0 INZ(4)
     C     A             COMP      B                                  102030
     C   20              EXSR      SHOW
     C  N10              GOTO      DONE
     C                   SETON                                        40
     C     DONE          TAG
     C                   EVAL      *IN(41) = *IN20
     C                   SETON                                        LR
     C     SHOW          BEGSR
     C                   EVAL      LINE = 'a<b ' + *IN20 + *IN41
     C                   WRITE     QSYSPRT       LINE
     C                   ENDSR
