**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds lda dtaara(*lda) qualified;
  part char(10);
  num zoned(5:0);
end-ds;
lda.part = 'local';
lda.num = 123;
out lda;
clear lda;
in *dtaara;
line = 'lda [' + lda.part + '] ' + %char(lda.num);
write QSYSPRT line;
*inlr = *on;
