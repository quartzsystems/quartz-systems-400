     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DX                S              5  0
      /free
       x = 5;
      /end-free
     C                   EVAL      X = X + 1
      /FREE
       line = 'x=' + %char(x);
       write QSYSPRT line;
      /END-FREE
     C                   EVAL      *INLR = *ON
