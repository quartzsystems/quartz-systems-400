**FREE
//; setup: STRCMTCTL LCKLVL(*CHG) CMTSCOPE(*JOB)
//; cleanup: ENDCMTCTL
ctl-opt dftactgrp(*no) actgrp(*caller);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
commit(e);
line = 'commit ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
rolbk(e);
line = 'rolbk ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
commit(e) 'boundary1';
line = 'commit id ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
