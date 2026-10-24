**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) infsr(*pssr) infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fbstat *status;
  fbop *opcode;
end-ds;
dcl-ds pgmsts psds qualified;
  status *status;
  routine *routine;
end-ds;
dcl-s n int(10);
// INFSR(*PSSR) returning to *GETIN: the next cycle.
n += 1;
line = 'cycle ' + %char(n);
write QSYSPRT line;
if n >= 3;
  *inlr = *on;
  return;
endif;
update CUSTR;
line = 'after update';
write QSYSPRT line;
*inlr = *on;
begsr *pssr;
  line = 'pssr ' + %char(fbstat) + ' ' + fbop + ' ' + %char(%status) + ' '
       + %char(pgmsts.status) + ' ' + pgmsts.routine;
  write QSYSPRT line;
endsr '*GETIN';
