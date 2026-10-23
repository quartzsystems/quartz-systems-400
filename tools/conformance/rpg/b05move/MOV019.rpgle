     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DZ5               S              5S 0
     DC5               S              5A
     C                   MOVE      '00000'       Z5                   313233
     C                   EVAL      LINE = 'a ' + *IN31 + *IN32 + *IN33
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      '0001J'       Z5                   313233
     C                   EVAL      LINE = 'b ' + *IN31 + *IN32 + *IN33
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      '00012'       Z5                   313233
     C                   EVAL      LINE = 'c ' + *IN31 + *IN32 + *IN33
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      *BLANKS       C5                       34
     C                   EVAL      LINE = 'd ' + *IN34
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEL     'a'           C5                       34
     C                   EVAL      LINE = 'e ' + *IN34
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      -5            Z5                   313233
     C                   EVAL      LINE = 'f ' + *IN31 + *IN32 + *IN33
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
