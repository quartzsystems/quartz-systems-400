**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132) formlen(10) formofl(8) oflind(ovf);
dcl-ds line len(132) end-ds;
dcl-s i int(10);
dcl-s ovf ind;
for i = 1 to 14;
  if ovf;
    line = 'heading';
    write QSYSPRT line;
    ovf = *off;
  endif;
  line = 'line ' + %char(i);
  write QSYSPRT line;
endfor;
*inlr = *on;
