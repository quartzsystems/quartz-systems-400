**FREE
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f ORDF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// READPE backwards over one key; READE and READPE without a key.
setgt (101) ORDF;
readpe (101) ORDF;
dow not %eof(ORDF);
line = 'pe ' + %char(ORDCUS) + '/' + %char(ORDNO);
write QSYSPRT line;
readpe (101) ORDF;
enddo;
setll (107) ORDF;
read ORDF;
reade *key ORDF;
line = 'key ' + %char(ORDCUS) + '/' + %char(ORDNO) + ' ' + %eof(ORDF);
write QSYSPRT line;
reade *key ORDF;
line = 'key eof ' + %eof(ORDF);
write QSYSPRT line;
*inlr = *on;
