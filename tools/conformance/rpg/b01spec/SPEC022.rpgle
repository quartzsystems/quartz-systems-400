**FREE
ctl-opt dftactgrp(*no)
        actgrp(*new);
ctl-opt decedit(',')
        datedit(*dmy.);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n packed(7:2) inz(1234.5);
dcl-s d8 zoned(8:0) inz(25122026);
dcl-s d6 zoned(6:0) inz(251226);
line = %char(n) + ' ' + %editc(n : 'J');
write QSYSPRT line;
line = %editc(d8 : 'Y') + ' ' + %editc(d6 : 'Y');
write QSYSPRT line;
*inlr = *on;
