**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// A file error with a *PSSR but no INFSR.
line = 'before';
write QSYSPRT line;
update CUSTR;
line = 'after';
write QSYSPRT line;
*inlr = *on;
begsr *pssr;
  line = 'pssr ' + %char(%status);
  write QSYSPRT line;
  *inlr = *on;
endsr '*CANCL';
