     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DPTR              S               *
     DPTR2             S               *
     DN                S             10I 0 INZ(0)
     C                   ALLOC(E)  N             PTR
     C                   EVAL      LINE = 'alloc 0 ' + %CHAR(%STATUS) + ' ' +
     C                             %ERROR
     C                   WRITE     QSYSPRT       LINE
     C                   REALLOC(E)50            PTR2
     C                   EVAL      LINE = 'realloc null ' + %CHAR(%STATUS) + ' '
     C                             + %ERROR
     C                   WRITE     QSYSPRT       LINE
     C                   ALLOC     10            PTR
     C                   REALLOC(E)N             PTR
     C                   EVAL      LINE = 'realloc 0 ' + %CHAR(%STATUS) + ' ' +
     C                             %ERROR
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
