**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
commit(e);
line = 'commit ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
rolbk(e);
line = 'rolbk ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
monitor;
  commit;
  line = 'commit mon ok';
  write QSYSPRT line;
on-error;
  line = 'commit mon ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
