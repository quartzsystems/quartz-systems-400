     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DT                DS                  QUALIFIED TEMPLATE
     D ID                             5I 0 INZ(9)
     D NM                             6    INZ('tname')
     DX                DS                  LIKEDS(T)
     D                                     INZ(*LIKEDS)
     C                   EVAL      LINE = %CHAR(X.ID) + ' ' + X.NM + ' ' +
     C                             %CHAR(%SIZE(X))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
