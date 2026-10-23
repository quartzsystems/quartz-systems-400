     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /DEFINE ONE
      /IF DEFINED(ONE)
     C                   EVAL      LINE = 'one defined'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = 'one not defined'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /UNDEFINE ONE
      /IF NOT DEFINED(ONE)
     C                   EVAL      LINE = 'one gone'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*V7R3M0)
     C                   EVAL      LINE = 'at least 7.3'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*V9R9M0)
     C                   EVAL      LINE = 'never'
     C                   WRITE     QSYSPRT       LINE
      /ELSEIF DEFINED(*CRTBNDRPG)
     C                   EVAL      LINE = 'crtbndrpg'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
     C                   EVAL      *INLR = *ON
