     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DPK               S              7P 2 INZ(-12.5)
     DZN               S              5S 0 INZ(42)
     DBN               S              9B 0 INZ(123456)
     DIN               S             10I 0 INZ(-7)
     DUN               S              5U 0 INZ(65535)
     DCH               S             10    INZ('abc')
     DVC               S             20A   VARYING INZ('vary')
     DNUM              S              5  1 INZ(3.5)
     DREC              DS
     D ID                      1      5S 0
     D NM                      6     15
     D AMT                    16     19P 2
     C                   EVAL      ID = 12
     C                   EVAL      NM = 'Name'
     C                   EVAL      AMT = 99.99
     C                   EVAL      LINE = 'PK=' + %CHAR(PK) + ' ZN=' + %CHAR(ZN)
     C                             + ' BN=' + %CHAR(BN)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'IN=' + %CHAR(IN) + ' UN=' + %CHAR(UN)
     C                             + ' NUM=' + %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'CH=' + CH + '|' + ' VC=' + VC + '|'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'REC=' + REC
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'ID=' + %CHAR(ID) + ' AMT=' +
     C                             %CHAR(AMT)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
