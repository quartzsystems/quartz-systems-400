**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; post: STRCMTCTL LCKLVL(*CHG) CMTSCOPE(*JOB)
//; cleanup: ENDCMTCTL
//; cleanup: ENDJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output:*delete) commit;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// COMMIT and ROLBK of the changes to a file under commitment control.
chain (101) CUSTF;
BAL = 1;
update CUSTR;
rolbk;
chain(n) (101) CUSTF;
line = 'rolbk update ' + %editc(BAL:'X') + ' ' + %char(%status);
write QSYSPRT line;
chain (102) CUSTF;
BAL = 2;
update CUSTR;
commit;
rolbk;
chain(n) (102) CUSTF;
line = 'commit update ' + %editc(BAL:'X');
write QSYSPRT line;
CUSNO = 109;
NAME = 'New';
CITY = 'Nowhere';
BAL = 0;
write CUSTR;
rolbk;
chain(n) (109) CUSTF;
line = 'rolbk write ' + %char(%found(CUSTF));
write QSYSPRT line;
chain (103) CUSTF;
delete CUSTR;
rolbk;
chain(n) (103) CUSTF;
line = 'rolbk delete ' + %char(%found(CUSTF)) + ' ' + NAME;
write QSYSPRT line;
commit(e);
line = 'empty commit ' + %char(%status) + ' ' + %char(%error);
write QSYSPRT line;
*inlr = *on;
