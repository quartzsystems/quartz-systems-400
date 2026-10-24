**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s p packed(5:2);
dcl-s i int(5);
dcl-s u uns(3);
dcl-s z zoned(3:0);
p = %hival(p);
i = %loval(i);
line = 'p ' + %editc(p:'X') + ' i ' + %char(i) + ' ' + %char(%hival(i));
write QSYSPRT line;
line = 'u ' + %char(%hival(u)) + ' ' + %char(%loval(u)) + ' z ' + %char(%loval(z));
write QSYSPRT line;
*inlr = *on;
