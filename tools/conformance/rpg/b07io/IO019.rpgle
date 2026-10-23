**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// (N): reads without a lock; UNLOCK; an update needs a locked record.
chain(n) (101) CUSTF;
update(e) CUSTR;
line = 'nolock update ' + %char(%status(CUSTF));
write QSYSPRT line;
chain (101) CUSTF;
unlock CUSTF;
update(e) CUSTR;
line = 'unlocked update ' + %char(%status(CUSTF));
write QSYSPRT line;
chain (101) CUSTF;
BAL = 1;
update CUSTR;
chain(n) (101) CUSTF;
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
*inlr = *on;
