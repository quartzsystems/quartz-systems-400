**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CHGPF FILE({lib}/CUSTF) WAITRCD(1)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update);
dcl-f CUST2 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C2R) prefix(X_);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// A record lock nothing handles ends the program.
chain (105) CUSTF;
line = 'locked ' + NAME;
write QSYSPRT line;
chain (105) CUST2;
line = 'not reached';
write QSYSPRT line;
*inlr = *on;
