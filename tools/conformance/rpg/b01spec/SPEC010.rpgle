      * A comment line
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132

     DV                S              5  0                                      comment area
       // a free-form comment line
     C                   EVAL      V = 9
     C                   EVAL      LINE = 'v=' + %CHAR(V)                       trailing words
     C                   WRITE     QSYSPRT       LINE
      * last comment
     C                   EVAL      *INLR = *ON
