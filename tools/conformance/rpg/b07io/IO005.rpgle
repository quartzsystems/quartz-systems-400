**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// SETLL and SETGT set %FOUND and %EQUAL.
setll (104) CUSTF;
line = 'll 104 f=' + %found(CUSTF) + ' e=' + %equal(CUSTF) + ' ' + %found + %equal;
write QSYSPRT line;
setll (100) CUSTF;
line = 'll 100 f=' + %found(CUSTF) + ' e=' + %equal(CUSTF);
write QSYSPRT line;
setll (109) CUSTF;
line = 'll 109 f=' + %found(CUSTF) + ' e=' + %equal(CUSTF);
write QSYSPRT line;
setgt (107) CUSTF;
line = 'gt 107 f=' + %found(CUSTF);
write QSYSPRT line;
setgt (108) CUSTF;
line = 'gt 108 f=' + %found(CUSTF);
write QSYSPRT line;
if %equal(CUSTF);
line = 'still equal';
else;
line = 'not equal';
endif;
write QSYSPRT line;
*inlr = *on;
