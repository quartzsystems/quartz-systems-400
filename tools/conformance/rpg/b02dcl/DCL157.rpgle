**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s u1 ucs2(5) inz(u'00680065006C006C006F');
dcl-s u2 varucs2(10) inz(u'00480049');
dcl-s c1 char(5);
dcl-s v1 varchar(10);
c1 = %char(u1);
v1 = %char(u2);
u2 = u2 + %ucs2('!');
line = c1 + ' ' + v1 + ' ' + %char(u2) + ' ' + %char(%len(u2)) + ' ' + %char(%size(u1));
write QSYSPRT line;
*inlr = *on;
