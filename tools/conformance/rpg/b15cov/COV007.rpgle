     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S             10I 0 INZ(41)
     DPP               S               *   PROCPTR INZ(%PADDR('TWICE'))
     C                   CALLB     'ADDONE'
     C                   PARM                    N
     C                   EVAL      LINE = 'addone ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   CALLB     PP
     C                   PARM                    N
     C                   EVAL      LINE = 'twice ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     PADDONE           B
     DADDONE           PI
     D X                             10I 0
     C                   EVAL      X = X + 1
     PADDONE           E
     PTWICE            B
     DTWICE            PI
     D X                             10I 0
     C                   EVAL      X = X * 2
     PTWICE            E
