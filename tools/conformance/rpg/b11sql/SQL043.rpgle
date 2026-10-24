**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; with: SQL043I
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// INCLUDE of a member that holds embedded statements too.
line = %char(countOf('ACC')) + ' ' + %char(countOf('DEV')) + ' ' + %char(countOf('XYZ'));
write QSYSPRT line;
*inlr = *on;
exec sql include SQL043I;
