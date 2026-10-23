     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DR5               S              5A
     DS                S              8A   INZ('abcdefgh')
     DL                S              3P 0 INZ(2)
     DST               S              3P 0 INZ(6)
     DV                S             10A   VARYING
     C                   EVAL      R5 = 'xxxxx'
     C     3             SUBST     S:2           R5
     C                   EVAL      LINE = 'a [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = 'xxxxx'
     C     3             SUBST(P)  S:2           R5
     C                   EVAL      LINE = 'b [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = 'xxxxx'
     C                   SUBST     S:6           R5
     C                   EVAL      LINE = 'c [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = 'xxxxx'
     C     L             SUBST     S:ST          R5
     C                   EVAL      LINE = 'd [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = 'xxxxx'
     C     8             SUBST     S             R5
     C                   EVAL      LINE = 'e [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = 'xxxxx'
     C     2             SUBST     S:1           R5                     40
     C                   EVAL      LINE = 'h [' + R5 + '] ' + *IN40
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V = 'abc'
     C     4             SUBST     S:3           V
     C                   EVAL      LINE = 'i [' + V + '] ' + %CHAR(%LEN(V))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      R5 = 'xxxxx'
     C                   SUBST     'pqrst':3     R5
     C                   EVAL      LINE = 'j [' + R5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
