**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s u uns(10);
dcl-s p packed(5:2) inz(12.50);
dcl-s c char(8) inz(' 42.7 ');
u = %uns(p);
line = 'a ' + %char(u) + ' ' + %char(%unsh(p));
write QSYSPRT line;
u = %uns(c);
line = 'b ' + %char(u) + ' ' + %char(%unsh(c));
write QSYSPRT line;
line = 'c ' + %char(%uns(7.999)) + ' ' + %char(%unsh(7.5)) + ' ' + %char(%unsh(7.49));
write QSYSPRT line;
*inlr = *on;
