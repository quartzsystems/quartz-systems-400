**FREE
ctl-opt dftactgrp(*no) ccsid(*char:37);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(10) inz('abc');
dcl-s u varchar(10) ccsid(*utf8);
dcl-s h char(10) ccsid(*hex);
u = c;
h = c;
c = u;
line = '[' + c + '] ' + %char(%len(u)) + ' ' + %char(%size(u));
write QSYSPRT line;
*inlr = *on;
