**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*CHG) OUTPUT(*PRINT)
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
// A row an SQL statement changed stays locked to the commit, against
// the program's own file opened without commitment control.
exec sql update EMPT set SAL = 3 where ID = 1;
line = 'update ' + %char(sqlcode);
write QSYSPRT line;
chain(e) 1 EMPT;
line = 'native ' + %char(%status(EMPT));
write QSYSPRT line;
exec sql select int(SAL) into :n from EMPT where ID = 3;
chain(e) 3 EMPT;
line = 'read row ' + %char(%status(EMPT));
write QSYSPRT line;
unlock EMPT;
exec sql commit;
chain(e) 1 EMPT;
line = 'after commit ' + %char(%status(EMPT)) + ' ' + %char(%int(E_SAL));
write QSYSPRT line;
unlock EMPT;
*inlr = *on;
