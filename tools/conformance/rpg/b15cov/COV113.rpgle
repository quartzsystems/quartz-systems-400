**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds c extname('CUSTF') qualified;
  who extfld('NOFIELD');
end-ds;
line = c.name;
write QSYSPRT line;
*inlr = *on;
