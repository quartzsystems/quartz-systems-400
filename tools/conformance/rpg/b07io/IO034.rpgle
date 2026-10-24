**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: STRCMTCTL LCKLVL(*CHG) CMTSCOPE(*JOB)
//; cleanup: ENDCMTCTL
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update) commit usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// A COMMIT file that is not journaled.
open(e) CUSTF;
line = 'open ' + %char(%status) + ' ' + %char(%open(CUSTF));
write QSYSPRT line;
if %open(CUSTF);
  chain (101) CUSTF;
  BAL = 7;
  update(e) CUSTR;
  line = 'update ' + %char(%status);
  write QSYSPRT line;
  close CUSTF;
endif;
*inlr = *on;
