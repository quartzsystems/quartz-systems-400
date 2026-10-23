**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s s varchar(30) inz('the cat sat');
line = %replace('dog':s:5:3);
write QSYSPRT line;
line = %replace('big ':s:5:0);
write QSYSPRT line;
line = %replace('':s:4:4);
write QSYSPRT line;
line = %replace('X':s);
write QSYSPRT line;
line = %upper('mixed Case') + ' ' + %lower('MIXED Case');
write QSYSPRT line;
*inlr = *on;
