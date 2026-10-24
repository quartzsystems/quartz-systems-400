**FREE
ctl-opt dftactgrp(*no) decprec(30);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a packed(30:0) inz(999999999999999999999999999999);
dcl-s b packed(5:2) inz(1.5);
line = %char(%len(%char(a * b)));
write QSYSPRT line;
monitor;
  a = a * b;
  line = 'mult ok';
  write QSYSPRT line;
on-error;
  line = 'mult ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
