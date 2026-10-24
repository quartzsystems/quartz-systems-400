     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DABBR             S              3    DIM(4) CTDATA PERRCD(1)
     DFULL             S             10    DIM(4) ALT(ABBR)
     DI                S              3  0
     C     1             DO        4             I
     C                   EVAL      LINE = ABBR(I) + '=' + %TRIM(FULL(I))
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C                   EVAL      *INLR = *ON
**CTDATA ABBR
JANJanuary
FEBFebruary
MARMarch
APRApril
