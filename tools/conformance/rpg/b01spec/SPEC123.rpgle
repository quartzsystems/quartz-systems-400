     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   EVAL      LINE = *INU1 + *INU2 + *INU3 + *INU4 + *INU5
     C                             + *INU6 + *INU7 + *INU8
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
