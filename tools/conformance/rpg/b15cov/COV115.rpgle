      *; dds: MSTF
      *; sqlfile: MSTF
     HDFTACTGRP(*NO)
     FMSTF      IT   F   13        DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DARR              S             13    DIM(4) FROMFILE(MSTF) PERRCD(1)
     DI                S             10I 0
     C     1             DO        4             I
     C                   EVAL      LINE = 'arr ' + ARR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C                   EVAL      *INLR = *ON
