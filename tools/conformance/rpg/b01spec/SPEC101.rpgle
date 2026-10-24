      *; with: SPC101A, SPC101B
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /COPY SPC101A
      /COPY SPC101A
     C                   EVAL      LINE = %CHAR(GUARD) + ' ' + INNER
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
