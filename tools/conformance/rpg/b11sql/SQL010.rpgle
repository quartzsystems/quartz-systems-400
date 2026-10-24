**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n int(10);
dcl-s p packed(7:2);
dcl-s c varchar(30);
dcl-s a char(10) inz('abc');
// VALUES INTO and SET :hv.
exec sql values 6 * 7 into :n;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql set :p = 10.5 / 4;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(p);
write QSYSPRT line;
exec sql set :c = upper(:a) concat 'X';
line = %char(sqlcode) + ' ' + sqlstate + ' ' + c + ' ' + %char(%len(c));
write QSYSPRT line;
exec sql values (length(trim(:a)), substr('Hello', 2, 3)) into :n, :c;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n) + ' ' + c;
write QSYSPRT line;
exec sql set :n = coalesce(cast(null as int), 5);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql set :p = :p * 2, :n = :n + 1;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(p) + ' ' + %char(n);
write QSYSPRT line;
*inlr = *on;
