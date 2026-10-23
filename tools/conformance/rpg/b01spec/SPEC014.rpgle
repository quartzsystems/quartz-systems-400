     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   EVAL      LINE = 'before eof'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
      /EOF
this is not RPG at all
     C                   EVAL      LINE = 'never'
