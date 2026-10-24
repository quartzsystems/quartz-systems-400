     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DI                S              3  0
     C                   DOW       I < 10
     C                   EVAL      I = I + 1
     C                   IF        %REM(I:2) = 0
     C                   ITER
     C                   ENDIF
     C                   IF        I > 7
     C                   LEAVE
     C                   ENDIF
     C                   EVAL      LINE = 'i=' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C                   DOU       I >= 12
     C                   EVAL      I = I + 2
     C                   ENDDO
     C                   EVAL      LINE = 'end ' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
