**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds pgmsts psds qualified;
  status *status;
  excid char(7) pos(40);
end-ds;
// MONITOR takes file errors: by status, *FILE and *ALL.
monitor;
  read CUSTF;
on-error 1221;
  line = 'no 1221';
  write QSYSPRT line;
on-error 1211;
  line = 'closed ' + %char(%status) + ' ' + %char(%status(CUSTF)) + ' ' + %error;
  write QSYSPRT line;
endmon;
open CUSTF;
monitor;
  update CUSTR;
on-error *program;
  line = 'program ' + %char(%status);
  write QSYSPRT line;
on-error *file;
  line = 'file ' + %char(%status) + ' ' + %char(pgmsts.status) + ' ' + pgmsts.excid;
  write QSYSPRT line;
endmon;
monitor;
  CUSNO = 104;
  write CUSTR;
on-error *all;
  line = 'all ' + %char(%status) + ' ' + %char(%status(CUSTF));
  write QSYSPRT line;
endmon;
monitor;
  open CUSTF;
on-error 1215;
  line = 'reopen ' + %char(%status);
  write QSYSPRT line;
endmon;
line = 'end ' + %char(%status) + ' ' + %error;
write QSYSPRT line;
*inlr = *on;
