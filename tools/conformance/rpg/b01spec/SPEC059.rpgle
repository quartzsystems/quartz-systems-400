     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3  0
     C     1             DO        3             N
     C     N             CASEQ     1             SR1
     C     N             CASEQ     2             SR2
     C                   CAS                     SR3
     C                   ENDCS
     C                   ENDDO
     C                   EVAL      *INLR = *ON
     C     SR1           BEGSR
     C                   EVAL      LINE = 'sr1'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDSR
     C     SR2           BEGSR
     C                   EVAL      LINE = 'sr2'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDSR
     C     SR3           BEGSR
     C                   EVAL      LINE = 'sr3 ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDSR
