**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CHGPF FILE({lib}/CUSTF) WAITRCD(1)
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; post: STRCMTCTL LCKLVL(*CHG) CMTSCOPE(*JOB)
//; cleanup: ENDCMTCTL
//; cleanup: ENDJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update) commit;
dcl-f CUST2 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C2R) prefix(X_);
dcl-f CUST3 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C3R) prefix(Y_) commit;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// Record locks under LCKLVL(*CHG): a changed record stays locked to the
// commit, a record only read until the next operation.
chain (101) CUSTF;
BAL = 21;
update CUSTR;
chain(e) (101) CUST2;
line = 'changed ' + %char(%status(CUST2));
write QSYSPRT line;
chain(e) (101) CUST3;
line = 'same definition ' + %char(%status(CUST3));
write QSYSPRT line;
unlock CUST3;
commit;
chain(e) (101) CUST2;
line = 'after commit ' + %char(%status(CUST2)) + ' ' + %editc(X_BAL:'X');
write QSYSPRT line;
unlock CUST2;
chain (104) CUSTF;
chain(e) (104) CUST2;
line = 'read ' + %char(%status(CUST2));
write QSYSPRT line;
chain (105) CUSTF;
chain(e) (104) CUST2;
line = 'next read ' + %char(%status(CUST2));
write QSYSPRT line;
unlock CUST2;
commit;
*inlr = *on;
