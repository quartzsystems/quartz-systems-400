     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DI                S              3  0
     C                   FOR       I = 1 TO 10 BY 3
     C                   EVAL      LINE = 'up ' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDFOR
     C                   FOR       I = 5 DOWNTO 1 BY 2
     C                   EVAL      LINE = 'down ' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDFOR
     C                   EVAL      *INLR = *ON
