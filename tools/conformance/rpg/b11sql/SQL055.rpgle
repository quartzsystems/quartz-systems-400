**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*CS) OUTPUT(*PRINT)
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
// COMMIT(*CS): the row a cursor stands on, against the program's own
// file.
exec sql declare c1 cursor for select ID from EMPT where ID in (1, 3, 5)
  order by ID;
exec sql open c1;
exec sql fetch c1 into :n;
line = 'fetch ' + %char(sqlcode) + ' ' + %char(n);
write QSYSPRT line;
chain(e) 1 EMPT;
line = 'current ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
exec sql fetch c1 into :n;
chain(e) 1 EMPT;
line = 'left ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
exec sql close c1;
chain(e) 3 EMPT;
line = 'closed ' + %char(%status(EMPT)) + ' ' + fbmsg;
write QSYSPRT line;
unlock EMPT;
exec sql commit;
*inlr = *on;
