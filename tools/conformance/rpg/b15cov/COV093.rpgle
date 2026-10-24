**FREE
ctl-opt dftactgrp(*no) srtseq(*langidshr) langid('ENU');
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a char(1) inz('a');
dcl-s b char(1) inz('A');
dcl-s c char(1) inz('B');
dcl-s r1 ind;
dcl-s r2 ind;
dcl-s r3 ind;
r1 = (a = b);
r2 = (a < c);
r3 = (c > a);
line = 'eq ' + r1 + ' lt ' + r2 + ' gt ' + r3;
write QSYSPRT line;
*inlr = *on;
