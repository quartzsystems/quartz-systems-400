     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D ds1             DS
     D  f1                     1      5
     D  f2                     6     10  2
     D  f3                    11     14B 0
     D  f4                    15     22P 3
     D ds2             DS                  INZ
     D  g1                            3
     D  g2                            5S 1
     D  g3                           10I 0
     D ds3             DS                  QUALIFIED
     D  h1                            4    INZ('h1h1')
     D  h2                                 LIKE(h1)
      /free
        f1 = 'abcde';
        f2 = 123.45;
        f3 = -77;
        f4 = 12345.678;
        line = f1 + ' ' + %char(f2) + ' ' + %char(f3) + ' ' + %char(f4) + ' ' +
        %char(%size(ds1));
        write QSYSPRT line;
        line = '[' + g1 + '] ' + %char(g2) + ' ' + %char(g3) + ' ' +
        %char(%size(ds2));
        write QSYSPRT line;
        line = ds3.h1 + '[' + ds3.h2 + '] ' + %char(%size(ds3));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
