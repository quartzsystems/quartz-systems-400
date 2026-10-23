     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC5               S              5A
     C                   EVAL      C5 = '12345'
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'a ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '  123'
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'b ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '     '
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'c ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '12 45'
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'd ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '1234J'
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'e ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'ab123'
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'f ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '123  '
     C                   TESTN                   C5                   414243
     C                   EVAL      LINE = 'g ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'A'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'h ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'J'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'i ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '1'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'j ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '&'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'k ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '-'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'l ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '}'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'm ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = '{'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'n ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'a'
     C                   TESTZ                   C5                   414243
     C                   EVAL      LINE = 'o ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
