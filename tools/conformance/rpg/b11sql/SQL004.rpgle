**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s id int(10) inz(6);
dcl-s nm varchar(20) inz('Fay');
dcl-s dp char(3) inz('DEV');
dcl-s sal packed(9:2) inz(45000.5);
dcl-s n int(10);
// INSERT: host variables, a duplicate key, two rows, a null.
exec sql insert into EMPT (ID, NAME, DEPT, SAL) values (:id, :nm, :dp, :sal);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql insert into EMPT (ID, NAME, DEPT, SAL) values (:id, :nm, :dp, :sal);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql insert into EMPT (ID, NAME) values (7, 'Gus'), (8, 'Hal');
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(sqlerrd(3));
write QSYSPRT line;
exec sql select count(*), sum(SAL) into :n, :sal from EMPT;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n) + ' ' + %char(sal);
write QSYSPRT line;
exec sql insert into EMPT (ID, NAME) values (9, NULL);
line = %char(sqlcode) + ' ' + sqlstate;
write QSYSPRT line;
*inlr = *on;
