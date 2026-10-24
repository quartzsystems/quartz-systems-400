     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCOMPUTE_THE_TOTAL_OF_TWO_...
     DNUMBERS_PLEASE   PR            10I 0
     D                               10I 0 VALUE
     D                               10I 0 VALUE
       line = %char(COMPUTE_THE_TOTAL_OF_TWO_NUMBERS_PLEASE(40 : 2));
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     PCOMPUTE_THE_TOTAL_OF_TWO_...
     PNUMBERS_PLEASE   B
     D                 PI            10I 0
     DA                              10I 0 VALUE
     DB                              10I 0 VALUE
     C                   RETURN    A + B
     PCOMPUTE_THE_TOTAL_OF_TWO_...
     PNUMBERS_PLEASE   E
