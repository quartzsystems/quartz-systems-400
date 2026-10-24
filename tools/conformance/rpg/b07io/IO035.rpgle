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
dcl-s cm ind inz(*off);
dcl-f CUSTF keyed usage(*update) commit(cm) usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// COMMIT(flag): the flag at the open decides.
open CUSTF;
chain (101) CUSTF;
BAL = 11;
update CUSTR;
rolbk;
chain(n) (101) CUSTF;
line = 'flag off ' + %editc(BAL:'X');
write QSYSPRT line;
close CUSTF;
cm = *on;
open CUSTF;
chain (102) CUSTF;
BAL = 12;
update CUSTR;
rolbk;
chain(n) (102) CUSTF;
line = 'flag on ' + %editc(BAL:'X');
write QSYSPRT line;
close CUSTF;
*inlr = *on;
