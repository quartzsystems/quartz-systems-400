      *; with: SPC118C
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /COPY SPC118C
     DAFTER            S              5    INZ('after')
     C                   EVAL      LINE = BEFORE + ' ' + AFTER
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
