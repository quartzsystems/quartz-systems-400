      /TITLE A title for the listing
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /SPACE 2
     DN                S              3  0 INZ(1)
      /EJECT
     C                   EVAL      LINE = 'after directives ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
      /SPACE
     C                   EVAL      *INLR = *ON
