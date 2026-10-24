     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCNT              S              5  0 INZ(42)
     DNM               S              8    INZ('init')
     DDS1              DS
     D A                              3    INZ('aaa')
     D B                              3  0 INZ(7)
     C                   EVAL      CNT = 1
     C                   EVAL      NM = 'chg'
     C                   EVAL      A = 'zz'
     C                   EVAL      B = 9
     C                   EVAL      LINE = %CHAR(CNT) + ' ' + NM + ' ' + DS1
     C                   WRITE     QSYSPRT       LINE
     C                   CLEAR                   CNT
     C                   CLEAR                   NM
     C                   EVAL      LINE = %CHAR(CNT) + ' [' + NM + '] ' + DS1
     C                   WRITE     QSYSPRT       LINE
     C                   RESET                   CNT
     C                   RESET                   NM
     C                   RESET                   DS1
     C                   EVAL      LINE = %CHAR(CNT) + ' ' + NM + ' ' + DS1
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
