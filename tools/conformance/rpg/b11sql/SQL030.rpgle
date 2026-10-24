**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c varchar(30);
dcl-s w varchar(10);
dcl-s n int(10);
dcl-s b packed(7:2);
dcl-s m packed(11:4);
dcl-s k int(10);
dcl-s c2 varchar(30);
// Expressions and scalar functions in the select list.
exec sql select upper(NAME) concat '-' concat DEPT, length(NAME),
  case when BONUS is null then 'none' else 'some' end, coalesce(BONUS, 0),
  SAL / 12, mod(ID, 2) into :c, :n, :w, :b, :m, :k from EMPT where ID = 2;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c + ' ' + %char(n) + ' ' + w;
write QSYSPRT line;
line = %char(b) + ' ' + %char(m) + ' ' + %char(k);
write QSYSPRT line;
exec sql select lower(NAME), substr(DEPT, 2), trim(char(ID * 7)) into :c, :w, :c2
  from EMPT where ID = 5;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c + ' ' + w + ' ' + c2;
write QSYSPRT line;
*inlr = *on;
