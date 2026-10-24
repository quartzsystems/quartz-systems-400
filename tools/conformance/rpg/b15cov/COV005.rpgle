     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DPTR              S               *
     DBUF              S            300    BASED(PTR)
     DN                S             10I 0
     C                   ALLOC     100           PTR
     C                   EVAL      %SUBST(BUF:1:5) = 'hello'
     C                   REALLOC   300           PTR
     C                   EVAL      %SUBST(BUF:290:3) = 'end'
     C                   EVAL      LINE = 'alloc ' + %SUBST(BUF:1:5) + ' ' +
     C                             %SUBST(BUF:290:3)
     C                   WRITE     QSYSPRT       LINE
     C                   DEALLOC                 PTR
     C                   IF        PTR = *NULL
     C                   EVAL      LINE = 'null after dealloc'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   EVAL      *INLR = *ON
