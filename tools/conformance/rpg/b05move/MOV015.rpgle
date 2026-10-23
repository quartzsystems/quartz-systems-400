     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DDS1              DS
     DSA                       1      3
     DSZ                       4      8S 0
     DSB                       9     10
     DARR              S              4A   DIM(3)
     DI                S              3P 0 INZ(2)
     DZA               S              3S 0 DIM(2)
     C                   EVAL      DS1 = 'abc00042xy'
     C                   MOVE      'Q'           DS1
     C                   EVAL      LINE = 'a [' + DS1 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEL     'ZZ'          DS1
     C                   EVAL      LINE = 'b [' + DS1 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      '7'           SZ
     C                   EVAL      LINE = 'c ' + %CHAR(SZ) + ' ' +
     C                             %EDITC(SZ:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'd [' + DS1 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      ARR(1) = 'aaaa'
     C                   EVAL      ARR(2) = 'bbbb'
     C                   EVAL      ARR(3) = 'cccc'
     C                   MOVE      'X'           ARR(I)
     C                   MOVEL     'Y'           ARR(3)
     C                   EVAL      LINE = 'f ' + ARR(1) + ARR(2) + ARR(3)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      ZA(1) = 999
     C                   MOVE      ARR(I)        ZA(1)
     C                   EVAL      LINE = 'g ' + %CHAR(ZA(1)) + ' ' +
     C                             %EDITC(ZA(1):'X')
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      SZ            ARR(1)
     C                   EVAL      LINE = 'h [' + ARR(1) + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
