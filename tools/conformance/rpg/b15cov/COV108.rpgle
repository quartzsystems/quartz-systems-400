**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF keyed usage(*input) block(*no);
dcl-f CUST2 likefile(CUSTF) extfile('CUSTF') usropn;
dcl-ds r likerec(CUSTR);
open CUST2;
chain (103) CUST2 r;
line = 'cust2 ' + r.name;
write QSYSPRT line;
chain (105) CUSTF r;
line = 'custf ' + r.name;
write QSYSPRT line;
close CUST2;
*inlr = *on;
