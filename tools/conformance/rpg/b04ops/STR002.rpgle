     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DS10              S             10A   INZ('abcabcabc')
     DPOS              S              3P 0
     DARR              S              3P 0 DIM(5)
     DV                S             10A   VARYING INZ('xxbc')
     DST               S              3P 0 INZ(4)
     C     'bc'          SCAN      S10           POS                      50
     C                   EVAL      LINE = 'a ' + %CHAR(POS) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     'bc'          SCAN      S10:3         POS                      50
     C                   EVAL      LINE = 'b ' + %CHAR(POS) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     'zz'          SCAN      S10           POS                      50
     C                   EVAL      LINE = 'c ' + %CHAR(POS) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     'bcx':2       SCAN      S10           POS
     C                   EVAL      LINE = 'd ' + %CHAR(POS)
     C                   WRITE     QSYSPRT       LINE
     C     'b'           SCAN      S10           ARR
     C                   EVAL      LINE = 'e ' + %CHAR(ARR(1)) + ' ' +
     C                             %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3)) + ' ' +
     C                             %CHAR(ARR(4)) + ' ' + %CHAR(ARR(5))
     C                   WRITE     QSYSPRT       LINE
     C     'c'           SCAN      S10                                    51
     C                   EVAL      LINE = 'f ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     'q'           SCAN      S10                                    51
     C                   EVAL      LINE = 'g ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     'bc'          SCAN      V             POS
     C                   EVAL      LINE = 'h ' + %CHAR(POS)
     C                   WRITE     QSYSPRT       LINE
     C     'a'           SCAN      S10:ST        POS
     C                   EVAL      LINE = 'i ' + %CHAR(POS)
     C                   WRITE     QSYSPRT       LINE
     C     ' '           SCAN      S10           POS
     C                   EVAL      LINE = 'j ' + %CHAR(POS)
     C                   WRITE     QSYSPRT       LINE
     C     'c'           SCAN      S10:4         ARR
     C                   EVAL      LINE = 'k ' + %CHAR(ARR(1)) + ' ' +
     C                             %CHAR(ARR(2)) + ' ' + %CHAR(ARR(3))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
