     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DZ3               S              3S 0 DIM(3)
     DZ2               S              3S 0 DIM(4)
     DZC               S              2A   DIM(4)
     DC6               S              6A
     C                   SETOFF                                       101519
     C                   SETOFF                                       202199
     C                   EVAL      Z3(1) = 123
     C                   EVAL      Z3(2) = 456
     C                   EVAL      Z3(3) = -789
     C                   EVAL      Z2 = 5
     C                   MOVEA     Z3            Z2(2)
     C                   EVAL      LINE = 'a ' + %EDITC(Z2(1):'X') +
     C                             %EDITC(Z2(2):'X') + %EDITC(Z2(3):'X') +
     C                             %EDITC(Z2(4):'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'b ' + %CHAR(Z2(1)) + ' ' +
     C                             %CHAR(Z2(4))
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEA     '0110'        *IN(11)
     C                   EVAL      LINE = 'c ' + *IN10 + *IN11 + *IN12 + *IN13 +
     C                             *IN14 + *IN15
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C6 = '......'
     C                   MOVEA     *IN(11)       C6
     C                   EVAL      LINE = 'd [' + C6 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEA     *ALL'1'       *IN(20)
     C                   EVAL      LINE = 'e ' + *IN19 + *IN20 + *IN21 + *IN99
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEA(P)  '1'           *IN(20)
     C                   EVAL      LINE = 'f ' + *IN20 + *IN21 + *IN99
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
