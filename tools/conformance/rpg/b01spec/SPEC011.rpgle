     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DMONTHS           S              3    DIM(12) CTDATA PERRCD(6)
     DCODES            S              2  0 DIM(3) CTDATA PERRCD(3)
     DI                S              3  0
     C                   Z-ADD     1             I
     C     I             DOWLE     12
     C                   EVAL      LINE = %CHAR(I) + '=' + MONTHS(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ADD       1             I
     C                   ENDDO
     C                   EVAL      LINE = %CHAR(CODES(1)) + ' ' +
     C                             %CHAR(CODES(2)) + ' ' + %CHAR(CODES(3))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA MONTHS
JANFEBMARAPRMAYJUN
JULAUGSEPOCTNOVDEC
**CTDATA CODES
010203
