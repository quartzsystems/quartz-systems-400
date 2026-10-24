**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*ALL) OUTPUT(*PRINT)
//; sqlfile: EMPT
//; post: CHGPF FILE({lib}/EMPT) WAITRCD(1)
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/EMPT) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; cleanup: ENDJRNPF FILE({lib}/EMPT) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f EMPT usage(*update) rename(EMPT:EMPR) prefix(E_);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
// COMMIT(*ALL): a row a query read stays locked to the commit.
exec sql select int(SAL) into :n from EMPT where ID = 3;
line = 'select ' + %char(sqlcode) + ' ' + %char(n);
write QSYSPRT line;
chain(e) 3 EMPT;
line = 'read row ' + %char(%status(EMPT));
write QSYSPRT line;
exec sql commit;
chain(e) 3 EMPT;
line = 'after commit ' + %char(%status(EMPT));
write QSYSPRT line;
unlock EMPT;
*inlr = *on;
