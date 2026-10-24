**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s cf char(20);
dcl-s vv varchar(5);
dcl-s dv varchar(10);
dcl-s n int(10);
dcl-s ind int(5);
// Fixed and varying character host variables.
exec sql select NAME into :cf from EMPT where ID = 1;
line = '<' + cf + '>';
write QSYSPRT line;
exec sql select DEPT into :dv from EMPT where ID = 1;
line = %char(sqlcode) + ' ' + sqlstate + ' <' + dv + '> ' + %char(%len(dv));
write QSYSPRT line;
exec sql values 'abcdefgh' into :vv :ind;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + vv + ' ' + %char(ind) + ' ' + sqlwn1;
write QSYSPRT line;
vv = 'Kim  ';
exec sql insert into EMPT (ID, NAME) values (11, :vv);
exec sql select length(NAME) into :n from EMPT where ID = 11;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
cf = 'Lou';
exec sql insert into EMPT (ID, NAME) values (12, :cf);
exec sql select length(NAME) into :n from EMPT where ID = 12;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql select count(*) into :n from EMPT where NAME = :cf;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
*inlr = *on;
