**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds lda dtaara(*auto : *usrctl : *lda) len(1024);
  first char(8) pos(101);
end-ds;
first = 'auto-lda';
out lda;
line = first + ' ' + %char(%size(lda));
write QSYSPRT line;
*inlr = *on;
