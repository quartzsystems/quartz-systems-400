**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) usropn infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb len(400);
end-ds;
dcl-ds pgmsts psds len(429);
end-ds;
dcl-s z packed(3:0) inz(0);
dcl-s r packed(5:0);
// The INFDS after errors taken by (E); the PSDS in *PSSR.
chain(e) (101) CUSTF;
line = 'closed ' + %char(%status) + ' |' + %subst(fb:38:23) + '|';
write QSYSPRT line;
open CUSTF;
CUSNO = 101;
write(e) CUSTR;
line = 'dup ' + %char(%status) + ' |' + %subst(fb:38:23) + '|';
write QSYSPRT line;
update(e) CUSTR;
line = 'noread ' + %char(%status) + ' |' + %subst(fb:38:23) + '|';
write QSYSPRT line;
r = 1 / z;
*inlr = *on;
begsr *pssr;
  line = 'pssr |' + %subst(pgmsts:40:7) + '|' + %subst(pgmsts:334:20) + '|';
  write QSYSPRT line;
  *inlr = *on;
  return;
endsr;
