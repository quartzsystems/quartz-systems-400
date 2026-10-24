**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10) inz(1);
select n;
when-is 'one';
  line = 'one';
  write QSYSPRT line;
endsl;
*inlr = *on;
