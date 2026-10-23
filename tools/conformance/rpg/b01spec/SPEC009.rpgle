     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DX                S              5  0
      /FREE
       x = 7 * 6;
       line = 'x=' + %char(x);
       write QSYSPRT line;
       *inlr = *on;
      /END-FREE
