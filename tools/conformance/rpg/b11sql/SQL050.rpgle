**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*CHG) OUTPUT(*PRINT)
//; sqlfile: EMPT
//; post: CRTJRNRCV JRNRCV({lib}/QSRCV1)
//; post: CRTJRN JRN({lib}/QSJRN) JRNRCV({lib}/QSRCV1)
//; post: STRJRNPF FILE({lib}/EMPT) JRN({lib}/QSJRN) IMAGES(*BOTH)
//; cleanup: ENDJRNPF FILE({lib}/EMPT) JRN({lib}/QSJRN)
//; cleanup: DLTJRN JRN({lib}/QSJRN)
//; cleanup: DLTJRNRCV JRNRCV({lib}/QSRCV1) DLTOPT(*IGNINQMSG)
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
// COMMIT(*CHG): ROLLBACK undoes the changes since the last COMMIT.
exec sql update EMPT set SAL = 1 where ID = 1;
line = 'update ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql rollback;
line = 'rollback ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql select int(SAL) into :n from EMPT where ID = 1;
line = 'after rollback ' + %char(n);
write QSYSPRT line;
exec sql update EMPT set SAL = 2 where ID = 2;
exec sql commit;
line = 'commit ' + %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
exec sql rollback;
exec sql select int(SAL) into :n from EMPT where ID = 2;
line = 'after commit ' + %char(n);
write QSYSPRT line;
exec sql insert into EMPT (ID, NAME) values (9, 'Zed');
exec sql delete from EMPT where ID = 3;
exec sql select count(*) into :n from EMPT;
line = 'before rollback ' + %char(n);
write QSYSPRT line;
exec sql rollback;
exec sql select count(*) into :n from EMPT;
line = 'rows ' + %char(n);
write QSYSPRT line;
exec sql select count(*) into :n from EMPT where ID = 3;
line = 'id 3 ' + %char(n);
write QSYSPRT line;
*inlr = *on;
