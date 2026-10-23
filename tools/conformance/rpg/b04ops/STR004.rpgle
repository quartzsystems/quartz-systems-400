     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DR10              S             10A
     DR5               S              5A
     DS                S              9A   INZ('aabbcc dd')
     DFROM             S              3A   INZ('abc')
     DTO               S              3A   INZ('XYZ')
     C                   EVAL      R10 = '**********'
     C     'abc':'ABC'   XLATE     S             R10
     C                   EVAL      LINE = 'a [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = '**********'
     C     FROM:TO       XLATE     S:4           R10
     C                   EVAL      LINE = 'b [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = '*****'
     C     FROM:TO       XLATE     S             R5
     C                   EVAL      LINE = 'c [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = '**********'
     C     FROM:TO       XLATE(P)  'abc'         R10
     C                   EVAL      LINE = 'd [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = '**********'
     C     FROM:TO       XLATE     'cab'         R10
     C                   EVAL      LINE = 'e [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = '**********'
     C     'abc':'X'     XLATE     S             R10
     C                   EVAL      LINE = 'f [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = '**********'
     C     ' ':'_'       XLATE     S             R10
     C                   EVAL      LINE = 'g [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'hello'
     C     'lo':'01'     XLATE     R10           R10
     C                   EVAL      LINE = 'h [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
