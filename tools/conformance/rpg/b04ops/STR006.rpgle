     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DR10              S             10A
     DR3               S              3A
     DV                S             10A   VARYING
     C                   EVALR     R10 = 'abc'
     C                   EVAL      LINE = 'a [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVALR     R3 = 'abcdef'
     C                   EVAL      LINE = 'b [' + R3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVALR     R10 = %TRIM('  xy  ')
     C                   EVAL      LINE = 'c [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVALR     R10 = 'abc  '
     C                   EVAL      LINE = 'd [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V = 'hello'
     C                   EVALR     R10 = V
     C                   EVAL      LINE = 'e [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVALR     R3 = V
     C                   EVAL      LINE = 'f [' + R3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVALR     R10 = *BLANKS
     C                   EVAL      LINE = 'h [' + R10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
