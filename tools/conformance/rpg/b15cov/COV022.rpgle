**FREE
//; setup: CRTDTAARA DTAARA({lib}/COVDA1) TYPE(*CHAR) LEN(20) VALUE('first value')
//; cleanup: DLTDTAARA DTAARA({lib}/COVDA1)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s da char(20) dtaara('COVDA1');
in da;
line = 'in [' + da + ']';
write QSYSPRT line;
in *lock da;
da = 'changed';
out da;
da = *blanks;
in da;
line = 'again [' + da + ']';
write QSYSPRT line;
*inlr = *on;
