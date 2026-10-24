**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s g1 graph(5);
dcl-s g3 varucs2(5 : 4);
dcl-s u1 ucs2(5);
dcl-s u2 varucs2(5) inz(u'00610062');
dcl-s c1 varchar(65535);
dcl-s c2 varchar(10 : 2);
line = %char(%size(g1)) + ' ' + %char(%size(g3)) + ' ' + %char(%size(u1));
write QSYSPRT line;
line = %char(%size(u2)) + ' ' + %char(%len(u2)) + ' ' + %char(%size(c1)) + ' ' +
  %char(%size(c2));
write QSYSPRT line;
*inlr = *on;
