**FREE
//; setup: CRTDTAARA DTAARA({lib}/COVDA2) TYPE(*CHAR) LEN(20) VALUE('auto value')
//; cleanup: DLTDTAARA DTAARA({lib}/COVDA2)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds da dtaara(*auto:'COVDA2') len(20);
end-ds;
line = 'auto [' + da + ']';
write QSYSPRT line;
da = 'updated';
*inlr = *on;
