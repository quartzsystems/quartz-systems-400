     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3P 0 INZ(1)
     C     N             CASEQ     1             NOSUCH
     C                   ENDCS
     C                   EVAL      LINE = 'after'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
