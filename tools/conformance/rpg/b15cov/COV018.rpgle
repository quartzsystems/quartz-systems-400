     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C     *LIKE         DEFINE    NOSUCH        NEWF             +1
     C                   EVAL      LINE = 'x'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
