**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed prefix(C_) rename(CUSTR:CUSREC);
dcl-f ORDF keyed prefix('O':1);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// PREFIX and RENAME.
chain (107) CUSREC;
line = %char(C_CUSNO) + ' ' + C_NAME + C_CITY;
write QSYSPRT line;
setll (C_CUSNO) ORDF;
reade (C_CUSNO) ORDR;
dow not %eof(ORDF);
line = %char(ORDCUS) + ' ' + %char(ORDNO) + ' ' + OTEM + %char(OTY) + ' ' + %char(ORICE);
write QSYSPRT line;
reade (C_CUSNO) ORDR;
enddo;
*inlr = *on;
