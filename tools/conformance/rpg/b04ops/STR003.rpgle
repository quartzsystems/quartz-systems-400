     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DDIG              S             10A   INZ('0123456789')
     DS                S              8A   INZ('12a4b6  ')
     DPOS              S              3P 0
     DARR              S              3P 0 DIM(4)
     DT                S              6A   INZ('  ab  ')
     C     DIG           CHECK     S             POS                      50
     C                   EVAL      LINE = 'a ' + %CHAR(POS) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     DIG           CHECK     S:4           POS                      50
     C                   EVAL      LINE = 'b ' + %CHAR(POS) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     DIG           CHECK     '1234'        POS                      50
     C                   EVAL      LINE = 'c ' + %CHAR(POS) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     DIG           CHECK     S             ARR
     C                   EVAL      LINE = 'd ' + %CHAR(ARR(1)) + ' ' +
     C                             %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3)) + ' ' +
     C                             %CHAR(ARR(4))
     C                   WRITE     QSYSPRT       LINE
     C     ' '           CHECKR    T             POS                      51
     C                   EVAL      LINE = 'e ' + %CHAR(POS) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     ' '           CHECKR    T:3           POS                      51
     C                   EVAL      LINE = 'f ' + %CHAR(POS) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     ' '           CHECKR    '   '         POS                      51
     C                   EVAL      LINE = 'g ' + %CHAR(POS) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     DIG           CHECKR    S             ARR
     C                   EVAL      LINE = 'h ' + %CHAR(ARR(1)) + ' ' +
     C                             %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3)) + ' ' +
     C                             %CHAR(ARR(4))
     C                   WRITE     QSYSPRT       LINE
     C     ' '           CHECK     T             POS
     C                   EVAL      LINE = 'i ' + %CHAR(POS)
     C                   WRITE     QSYSPRT       LINE
     C     'ab '         CHECK     T                                      52
     C                   EVAL      LINE = 'j ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
