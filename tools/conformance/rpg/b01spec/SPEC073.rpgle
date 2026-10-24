     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DR                S             10
     DS1               DS                  QUALIFIED
     D A                              3    INZ('one')
     D B                              5  0 INZ(5)
     D C                              2    INZ('cc')
     DS2               DS                  QUALIFIED
     D B                              7  2 INZ(1)
     D A                              5    INZ('x')
     D D                              1    INZ('d')
     C                   EVALR     R = 'abc'
     C                   EVAL-CORR S2 = S1
     C                   EVAL      LINE = '[' + R + '] ' + S2.A + ' ' +
     C                             %CHAR(S2.B) + ' ' + S2.D
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
