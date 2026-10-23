     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DR10              S             10A
     DC5               S              5A   INZ('ab')
     DV20              S             20A   VARYING
     DN                S              3P 0 INZ(2)
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     'ab'          CAT       'cd'          R10
     C                   EVAL      LINE = 'a [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     'ab'          CAT(P)    'cd'          R10
     C                   EVAL      LINE = 'b [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     C5            CAT       'cd'          R10
     C                   EVAL      LINE = 'c [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     C5            CAT       'cd':1        R10
     C                   EVAL      LINE = 'd [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     C5            CAT       'cd':0        R10
     C                   EVAL      LINE = 'e [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'hi'
     C                   CAT       'there':1     R10
     C                   EVAL      LINE = 'f [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     'abcdefgh'    CAT       'ijklmn'      R10
     C                   EVAL      LINE = 'g [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V20 = 'x'
     C     'ab'          CAT       'cd'          V20
     C                   EVAL      LINE = 'h [' + V20 + '] ' + %CHAR(%LEN(V20))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     'ab'          CAT       'cd':N        R10
     C                   EVAL      LINE = 'i [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     'ab'          CAT       '  cd':1      R10
     C                   EVAL      LINE = 'j [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     C5            CAT(P)    C5:2          R10
     C                   EVAL      LINE = 'k [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V20 = 'abc'
     C                   CAT       'de':1        V20
     C                   EVAL      LINE = 'l [' + V20 + '] ' + %CHAR(%LEN(V20))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R10 = 'xxxxxxxxxx'
     C     'abcdefghij'  CAT       'k':1         R10
     C                   EVAL      LINE = 'm [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
