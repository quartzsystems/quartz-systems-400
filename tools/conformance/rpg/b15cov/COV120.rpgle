**FREE
//; setup: CRTDTAARA DTAARA({lib}/COVDA3) TYPE(*CHAR) LEN(10) VALUE('abcdeFGHIJ')
//; cleanup: DLTDTAARA DTAARA({lib}/COVDA3)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds da dtaara('COVDA3') qualified;
  a char(5);
  b char(5);
end-ds;
in *lock da;
line = '[' + da.a + '] [' + da.b + ']';
write QSYSPRT line;
unlock da;
*inlr = *on;
