     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DSHORT            S             10    INZ('x')
     D                                     VARYING
     DA_VERY_LONG_FIELD_NAME_INDEED...
     D                 S              5  0 INZ(17)
     DARR              S              3  0 DIM(4)
     D                                     INZ(5)
     C                   EVAL      LINE = '[' + SHORT + '] ' +
     C                             %CHAR(A_VERY_LONG_FIELD_NAME_INDEED)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(ARR(1)) + ' ' + %CHAR(ARR(4)) +
     C                             ' ' + %CHAR(%ELEM(ARR))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
