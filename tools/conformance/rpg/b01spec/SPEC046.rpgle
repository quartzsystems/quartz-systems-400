      *; dds: CUSTF
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCUST           E DS                  EXTNAME(CUSTF)
     DCQ             E DS                  EXTNAME(CUSTF) QUALIFIED
     C                   EVAL      CUSNO = 5
     C                   EVAL      NAME = 'Ann'
     C                   EVAL      CQ.CUSNO = 6
     C                   EVAL      LINE = %CHAR(%SIZE(CUST)) + ' ' +
     C                             %CHAR(%SIZE(CQ)) + ' ' + %CHAR(%SIZE(CUSNO))
     C                             + ' ' + %CHAR(%SIZE(BAL))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(CUSNO) + ' ' + %TRIM(NAME) + ' '
     C                             + %CHAR(CQ.CUSNO)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
