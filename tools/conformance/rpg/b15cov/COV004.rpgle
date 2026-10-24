**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f QPRINT printer(132) infds(fb) usropn;
dcl-ds fb;
  fname *file;
  fsts *status;
  fop *opcode;
end-ds;
open QPRINT;
post QPRINT;
line = 'post ' + fname + ' ' + %char(fsts) + ' ' + fop;
write QSYSPRT line;
close QPRINT;
post(e) QPRINT;
line = 'closed ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
