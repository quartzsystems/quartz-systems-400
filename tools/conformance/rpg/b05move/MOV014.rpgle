     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC1               S              1A
     DC3               S              3A
     DZ1               S              1S 0
     DN1               S              1N
     C                   MOVE      '1'           *IN01
     C                   MOVE      '0'           *IN02
     C                   EVAL      LINE = 'a ' + *IN01 + *IN02
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C1 = 'x'
     C                   MOVE      *IN01         C1
     C                   EVAL      LINE = 'b [' + C1 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'xyz'
     C                   MOVE      *IN01         C3
     C                   EVAL      LINE = 'c [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'xyz'
     C                   MOVEL     *IN02         C3
     C                   EVAL      LINE = 'd [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      *ON           N1
     C                   EVAL      LINE = 'e ' + N1
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      *OFF          *IN01
     C                   EVAL      LINE = 'f ' + *IN01
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z1 = 5
     C                   MOVE      N1            Z1
     C                   EVAL      LINE = 'i ' + %CHAR(Z1) + ' ' +
     C                             %EDITC(Z1:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
