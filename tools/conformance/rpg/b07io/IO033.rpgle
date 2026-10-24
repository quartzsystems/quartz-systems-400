**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; cleanup: ENDJRNPF FILE({lib}/CUSTF) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update) commit usropn;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// A COMMIT file opened while commitment control is not started.
open(e) CUSTF;
line = 'open ' + %char(%status) + ' ' + %char(%open(CUSTF));
write QSYSPRT line;
commit(e);
line = 'commit ' + %char(%status);
write QSYSPRT line;
*inlr = *on;
