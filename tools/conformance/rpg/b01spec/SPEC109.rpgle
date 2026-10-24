     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCODES            S              2    DIM(4) CTDATA PERRCD(2)
     C                   EVAL      LINE = CODES(1) + CODES(2) + CODES(3) +
     C                             CODES(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA CODES
AABB  this rest is a comment
CCDD  so is this
