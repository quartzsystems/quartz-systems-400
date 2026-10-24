**FREE
//; dds: B8B
//; dds: B8J
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f B8B workstn ignore(FTR);
dcl-f B8J workstn include(FTR) usropn;
if 1 = 0;
  open B8J;
  exfmt HDR;
  write FTR;
endif;
line = 'x';
write QSYSPRT line;
*inlr = *on;
