**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CHGPF FILE({lib}/CUSTF) WAITRCD(1)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update);
dcl-f CUST2 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C2R) prefix(X_) usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds pgmsts psds qualified;
  status *status;
  msgid char(7) pos(40);
end-ds;
// A record lock taken by MONITOR: ON-ERROR 1218, *FILE.
open CUST2;
chain (101) CUSTF;
monitor;
  chain (101) CUST2;
  line = 'no error';
on-error 1218;
  line = 'on-error 1218 ' + %char(%status(CUST2)) + ' ' + %char(%status)
       + ' ' + %char(pgmsts.status) + ' ' + pgmsts.msgid;
endmon;
write QSYSPRT line;
setll *start CUST2;
monitor;
  read CUST2;
  line = 'no error';
on-error *file;
  line = 'on-error *file ' + %char(%status(CUST2)) + ' ' + %char(%status);
endmon;
write QSYSPRT line;
read(e) CUST2;
line = 'next ' + %char(%status(CUST2)) + ' ' + X_NAME;
write QSYSPRT line;
close CUST2;
*inlr = *on;
