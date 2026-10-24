**FREE
ctl-opt dftactgrp(*no) indent('| ') copynest(5) srtseq(*hex);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s i int(10);
for i = 1 to 2;
  if i = 2;
    line = 'indent ' + %char(i);
    write QSYSPRT line;
  endif;
endfor;
*inlr = *on;
