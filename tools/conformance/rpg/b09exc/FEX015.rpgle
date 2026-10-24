**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CHGPF FILE({lib}/CUSTF) WAITRCD(1)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update);
dcl-f CUST2 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C2R) prefix(X_) infds(fb2);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb2 len(400);
end-ds;
// Two opens of one file in a job: a record one holds for update.
chain (102) CUSTF;
line = 'first ' + %char(%status(CUSTF)) + ' ' + NAME;
write QSYSPRT line;
chain(e) (102) CUST2;
line = 'second ' + %char(%status(CUST2)) + ' ' + %char(%error) + ' '
     + %char(%found(CUST2)) + ' |' + %subst(fb2:38:15) + '|';
write QSYSPRT line;
chain(e) (103) CUST2;
line = 'other ' + %char(%status(CUST2)) + ' ' + X_NAME;
write QSYSPRT line;
chain(ne) (102) CUST2;
line = 'nolock ' + %char(%status(CUST2)) + ' ' + X_NAME;
write QSYSPRT line;
unlock CUSTF;
chain(e) (102) CUST2;
line = 'after unlock ' + %char(%status(CUST2)) + ' ' + X_NAME;
write QSYSPRT line;
chain(e) (102) CUSTF;
line = 'back ' + %char(%status(CUSTF));
write QSYSPRT line;
update C2R;
chain(e) (102) CUSTF;
line = 'after update ' + %char(%status(CUSTF)) + ' ' + NAME;
write QSYSPRT line;
read(e) CUST2;
line = 'read ' + %char(%status(CUST2)) + ' ' + X_NAME;
write QSYSPRT line;
*inlr = *on;
