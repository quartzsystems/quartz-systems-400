**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
//; sqlfile: DEPTT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s pat varchar(10) inz('%e%');
dcl-s nm varchar(20);
dcl-s dn varchar(15);
dcl-s n int(10);
dcl-s lo packed(9:2) inz(50000);
dcl-s hi packed(9:2) inz(60000);
// A join, LIKE, IN and BETWEEN with host variables.
exec sql declare c1 cursor for select E.NAME, D.DNAME from EMPT E join DEPTT D
  on E.DEPT = D.DEPT where E.NAME like :pat order by E.ID;
exec sql open c1;
dow 1 = 1;
  exec sql fetch c1 into :nm, :dn;
  if sqlcode <> 0;
    leave;
  endif;
  line = nm + ' ' + dn;
  write QSYSPRT line;
enddo;
exec sql close c1;
exec sql select count(*) into :n from EMPT where DEPT in ('ACC', 'OPS');
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql select count(*) into :n from EMPT where SAL between :lo and :hi;
line = %char(sqlcode) + ' ' + sqlstate + ' ' + %char(n);
write QSYSPRT line;
exec sql select DNAME into :dn from DEPTT where DEPT = (select DEPT from EMPT where ID = 4);
line = %char(sqlcode) + ' ' + sqlstate + ' ' + dn;
write QSYSPRT line;
*inlr = *on;
