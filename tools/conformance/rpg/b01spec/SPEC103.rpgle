     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /IF DEFINED(*ILERPG)
     C                   EVAL      LINE = '*ILERPG yes'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = '*ILERPG no'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*CRTBNDRPG)
     C                   EVAL      LINE = '*CRTBNDRPG yes'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = '*CRTBNDRPG no'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*CRTRPGMOD)
     C                   EVAL      LINE = '*CRTRPGMOD yes'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = '*CRTRPGMOD no'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*V7R5M0)
     C                   EVAL      LINE = '*V7R5M0 yes'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = '*V7R5M0 no'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*V6R1M0)
     C                   EVAL      LINE = '*V6R1M0 yes'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = '*V6R1M0 no'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
      /IF DEFINED(*V7R6M0)
     C                   EVAL      LINE = '*V7R6M0 yes'
     C                   WRITE     QSYSPRT       LINE
      /ELSE
     C                   EVAL      LINE = '*V7R6M0 no'
     C                   WRITE     QSYSPRT       LINE
      /ENDIF
     C                   EVAL      *INLR = *ON
