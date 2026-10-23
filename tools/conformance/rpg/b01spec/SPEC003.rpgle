     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL            S              9  2
     DTXT              S             40A   VARYING
     C                   EVAL      TOTAL = 1 +
     C                             2 +
     C                             3.25
     C                   EVAL      TXT = 'first part, +
     C                             second part'
     C                   EVAL      LINE = 'T=' + %CHAR(TOTAL) + ' [' + TXT + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      TXT = 'keep-
     C                             blanks'
     C                   EVAL      LINE = '[' + TXT + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
