     HDFTACTGRP(*NO) ALTSEQ(*SRC)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA                S              1    INZ('a')
     DZ                S              1    INZ('z')
     DX                S              1    INZ('a') ALTSEQ(*NONE)
     C     A             IFGT      Z
     C                   EVAL      LINE = 'a after z'
     C                   WRITE     QSYSPRT       LINE
     C                   ELSE
     C                   EVAL      LINE = 'a before z'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C     X             IFGT      Z
     C                   EVAL      LINE = 'x after z'
     C                   WRITE     QSYSPRT       LINE
     C                   ELSE
     C                   EVAL      LINE = 'x before z'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   EVAL      *INLR = *ON
**ALTSEQ
ALTSEQ    81AA
