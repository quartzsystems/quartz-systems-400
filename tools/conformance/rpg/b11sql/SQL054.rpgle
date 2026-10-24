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
dcl-f EMPT usage(*update) rename(EMPT:EMPR) prefix(E_) infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fbmsg char(7) pos(46);
end-ds;
dcl-s n int(10);
// COMMIT(*CHG): which rows a query leaves locked against the program's
// own file.
chain(e) 1 EMPT;
line = 'before sql ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
exec sql select int(SAL) into :n from EMPT where ID = 3;
chain(e) 3 EMPT;
line = 'row read ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
chain(e) 5 EMPT;
line = 'row not read ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
exec sql commit;
chain(e) 3 EMPT;
line = 'after commit ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
*inlr = *on;
