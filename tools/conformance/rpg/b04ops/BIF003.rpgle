**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s s char(8) inz('12x45 7');
dcl-c DIGITS '0123456789';
line = %char(%check(DIGITS:s)) + ' ' + %char(%checkr(DIGITS + ' ':s));
write QSYSPRT line;
line = %char(%check(DIGITS:'123')) + ' ' + %char(%check(' ':'   x'));
write QSYSPRT line;
line = %xlate('abc':'XYZ':'aabbcc');
write QSYSPRT line;
line = %xlate('0123456789':'##########':s);
write QSYSPRT line;
line = %xlate('a':'A':'banana':3);
write QSYSPRT line;
*inlr = *on;
