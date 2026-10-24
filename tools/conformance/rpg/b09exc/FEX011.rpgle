**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usropn extfile('NOSUCHF') extdesc('CUSTF');
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// OPEN of a file that does not exist.
monitor;
  open CUSTF;
on-error *file;
  line = 'open ' + %char(%status) + ' ' + %open(CUSTF);
  write QSYSPRT line;
endmon;
open CUSTF;
line = 'after';
write QSYSPRT line;
*inlr = *on;
