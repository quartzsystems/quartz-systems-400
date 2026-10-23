     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DI                S              3  0
     DJ                S              3  0 INZ(5)
     C     J             IFGT      3
     C     J             ANDLT     9
     C                   EVAL      LINE = 'J between'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C     J             IFEQ      1
     C     J             OREQ      5
     C                   EVAL      LINE = 'J is 1 or 5'
     C                   WRITE     QSYSPRT       LINE
     C                   ELSE
     C                   EVAL      LINE = 'J is other'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   Z-ADD     0             I
     C     I             DOWLT     3
     C                   ADD       1             I
     C                   EVAL      LINE = 'DOW ' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C     I             DOUGE     5
     C                   ADD       1             I
     C                   EVAL      LINE = 'DOU ' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C     I             CABEQ     5             SKIP
     C                   EVAL      LINE = 'not skipped'
     C                   WRITE     QSYSPRT       LINE
     C     SKIP          TAG
     C                   EVAL      LINE = 'after tag'
     C                   WRITE     QSYSPRT       LINE
     C                   GOTO      DONE
     C                   EVAL      LINE = 'not reached'
     C                   WRITE     QSYSPRT       LINE
     C     DONE          TAG
     C                   EVAL      *INLR = *ON
