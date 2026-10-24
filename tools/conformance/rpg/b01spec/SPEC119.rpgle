     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /DEFINE X1
      /IF DEFINED(X2)
     C                   EVAL      LINE = 'x2'
     C                   WRITE     QSYSPRT       LINE
      /ELSEIF NOT DEFINED(X3)
     C                   EVAL      LINE = 'not x3'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = 'else'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF NOT DEFINED(X1)
     C                   EVAL      LINE = 'no x1'
     C                   WRITE     QSYSPRT       LINE
      /ELSEIF DEFINED(X1)
     C                   EVAL      LINE = 'x1'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
     C                   EVAL      *INLR = *ON
