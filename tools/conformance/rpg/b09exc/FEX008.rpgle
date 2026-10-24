**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) infsr(errsr);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// MONITOR and INFSR together: which takes the error.
monitor;
  update CUSTR;
on-error;
  line = 'monitor ' + %char(%status);
  write QSYSPRT line;
endmon;
line = 'next';
write QSYSPRT line;
CUSNO = 105;
write CUSTR;
line = 'after';
write QSYSPRT line;
*inlr = *on;
begsr errsr;
  line = 'errsr ' + %char(%status);
  write QSYSPRT line;
  *inlr = *on;
endsr '*CANCL';
