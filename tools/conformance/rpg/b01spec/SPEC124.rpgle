**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(6);
dcl-s v varchar(6) inz('abc');
dcl-s z char(4);
dcl-s d date;
c = *allx'C1C2';
v = *all'xy';
z = *zeros;
d = *hival;
line = c + ' [' + v + '] ' + z + ' ' + %char(d);
write QSYSPRT line;
d = *loval;
c = *all'-';
line = %char(d) + ' ' + c;
write QSYSPRT line;
*inlr = *on;
