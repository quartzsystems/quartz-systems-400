      *; with: SPC117C
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /COPY SPC117C
     C                   EVAL      LINE = FRC + ' ' + %CHAR(FRDS.X)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
