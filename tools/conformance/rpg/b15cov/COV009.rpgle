     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3P 0
     DRES              S             20
     C     1             DO        4             N
     C     N             CASEQ     1             ONE
     C     N             CASGT     3             BIG
     C                   CAS                     REST
     C                   ENDCS
     C                   EVAL      LINE = %CHAR(N) + ' ' + RES
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C     N             CASLT     9             ONE                  515253
     C                   END
     C                   EVAL      LINE = 'ind ' + *IN51 + *IN52 + *IN53 + ' ' +
     C                             RES
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     C     ONE           BEGSR
     C                   EVAL      RES = 'one'
     C                   ENDSR
     C     BIG           BEGSR
     C                   EVAL      RES = 'big'
     C                   ENDSR
     C     REST          BEGSR
     C                   EVAL      RES = 'rest'
     C                   ENDSR
