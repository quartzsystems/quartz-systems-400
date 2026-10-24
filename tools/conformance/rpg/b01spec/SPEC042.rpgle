     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DBASE             S              7  2 INZ(1)
     DBIG              S             +3    LIKE(BASE)
     DSMALL            S             -2    LIKE(BASE)
     DNM               S              6    INZ('abc')
     DNM2              S             +4    LIKE(NM)
     C                   EVAL      LINE = %CHAR(%LEN(BIG)) + ' ' +
     C                             %CHAR(%DECPOS(BIG)) + ' ' + %CHAR(%SIZE(BIG))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(%LEN(SMALL)) + ' ' +
     C                             %CHAR(%DECPOS(SMALL)) + ' ' +
     C                             %CHAR(%SIZE(SMALL))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(%LEN(NM2)) + ' [' + NM2 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
