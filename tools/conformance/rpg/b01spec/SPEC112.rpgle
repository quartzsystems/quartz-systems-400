     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
       dcl-s total packed(9:2) inz(0);
       dcl-s i int(10);
       for i = 1 to 4;
             total = total                                                      a
                   + i * 1.5;                                                   b
       endfor;
       line =
          'total=' +
          %char(total);
       write QSYSPRT line;
       *inlr = *on;
