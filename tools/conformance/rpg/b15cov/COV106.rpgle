**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF disk usage(*input) extmbr('CUSTF') include(CUSTR) recno(rrn) infds(fb);
dcl-s rrn packed(10:0);
dcl-ds fb;
  frrn int(10) pos(397);
end-ds;
read CUSTF;
read CUSTF;
line = %char(CUSNO) + ' ' + %char(rrn) + ' ' + %char(frrn);
write QSYSPRT line;
chain 5 CUSTF;
line = 'chain ' + %char(CUSNO) + ' ' + %found(CUSTF);
write QSYSPRT line;
*inlr = *on;
