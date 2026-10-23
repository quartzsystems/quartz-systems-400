     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC5               S              5A
     DZ5               S              5S 0
     DP72              S              7P 2
     DC8               S              8A
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      123           C5
     C                   EVAL      LINE = 'a [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      -123          C5
     C                   EVAL      LINE = 'b [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVEL     -1.5          C5
     C                   EVAL      LINE = 'c [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      *BLANKS       C5
     C                   EVAL      LINE = 'd [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      *ZEROS        C5
     C                   EVAL      LINE = 'e [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      *ALL'xy'      C5
     C                   EVAL      LINE = 'f [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVEL     *ALL'xy'      C5
     C                   EVAL      LINE = 'g [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 12345
     C                   MOVE      '77'          Z5
     C                   EVAL      LINE = 'h ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 12345
     C                   MOVE      *ZEROS        Z5
     C                   EVAL      LINE = 'i ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 12345
     C                   MOVE      *ALL'3'       Z5
     C                   EVAL      LINE = 'j ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P72 = 11111.11
     C                   MOVE      -42           P72
     C                   EVAL      LINE = 'k ' + %CHAR(P72) + ' ' +
     C                             %EDITC(P72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P72 = 11111.11
     C                   MOVEL     42            P72
     C                   EVAL      LINE = 'l ' + %CHAR(P72) + ' ' +
     C                             %EDITC(P72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 1
     C                   MOVE      *HIVAL        Z5
     C                   EVAL      LINE = 'm ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 1
     C                   MOVE      *LOVAL        Z5
     C                   EVAL      LINE = 'n ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 12345
     C                   MOVE      *BLANKS       Z5
     C                   EVAL      LINE = 'o ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C8 = '12345678'
     C                   MOVEL     'ab'          C8
     C                   EVAL      LINE = 'p [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
