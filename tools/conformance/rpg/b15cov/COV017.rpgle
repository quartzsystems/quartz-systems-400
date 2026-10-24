     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DFLD              S              5    INZ('abcde')
     DNUM              S              5P 2 INZ(123.45)
     C     *LIKE         DEFINE    FLD           LONGER           +3
     C     *LIKE         DEFINE    NUM           SHORTN           -2
     C     *LIKE         DEFINE    FLD           SAME
     C                   EVAL      LONGER = FLD + 'xyz'
     C                   EVAL      SHORTN = 4.5
     C                   EVAL      LINE = LONGER + ' ' + %CHAR(%LEN(LONGER)) +
     C                             ' ' + %CHAR(%LEN(SHORTN)) + ' ' +
     C                             %EDITC(SHORTN:'X') + ' ' + %CHAR(%LEN(SAME))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
