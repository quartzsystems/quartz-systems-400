**FREE
ctl-opt dftactgrp(*no) intprec(10);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10) inz(100000);
dcl-s b int(10) inz(100000);
dcl-s c int(20);
monitor;
  c = a * b;
  line = 'mult ok';
  write QSYSPRT line;
on-error;
  line = 'mult ' + %char(%status);
  write QSYSPRT line;
endmon;
line = %char(c);
write QSYSPRT line;
*inlr = *on;
