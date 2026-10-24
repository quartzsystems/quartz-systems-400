**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CHGPF FILE({lib}/CUSTF) WAITRCD(1)
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; post: STRCMTCTL LCKLVL(*ALL) CMTSCOPE(*JOB)
//; cleanup: ENDCMTCTL
//; cleanup: ENDJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed commit;
dcl-f CUST2 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C2R) prefix(X_);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// LCKLVL(*ALL): a record an input file reads stays locked to the commit.
chain (106) CUSTF;
chain (107) CUSTF;
chain(e) (106) CUST2;
line = 'read ' + %char(%status(CUST2));
write QSYSPRT line;
commit;
chain(e) (106) CUST2;
line = 'after commit ' + %char(%status(CUST2));
write QSYSPRT line;
unlock CUST2;
*inlr = *on;
