**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f custf keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
chain
  (103)
  custr;
line = %trim(name);
write QSYSPRT line;
setll
  *end
  custf;
readp
  custf;
line = %trim(name);
write QSYSPRT line;
setll *start custf;
read custf;
line = %trim(name);
write QSYSPRT line;
*inlr = *on;
