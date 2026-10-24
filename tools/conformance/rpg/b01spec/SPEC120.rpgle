     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DDT               S               D
     DTM               S               T
     DTS               S               Z
     DHX               S              4
     C                   EVAL      DT = D'2026-07-04'
     C                   EVAL      TM = T'23.59.58'
     C                   EVAL      TS = Z'2026-07-04-23.59.58.000001'
     C                   EVAL      HX = X'C1C2C3C4'
     C                   EVAL      LINE = %CHAR(DT) + ' ' + %CHAR(TM) + ' ' + HX
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(TS)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
