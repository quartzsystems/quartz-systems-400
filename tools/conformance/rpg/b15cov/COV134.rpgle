**FREE
//; dds: B8A
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f B8A workstn maxdev(*file) saveds(keep) saveind(10) usropn;
dcl-ds keep;
  a char(5);
end-ds;
if 1 = 0;
  open B8A;
endif;
a = 'x';
line = 'saved ' + a;
write QSYSPRT line;
*inlr = *on;
